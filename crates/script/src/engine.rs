//! The `QuickJS` glue: a fresh runtime per run with the limits set, the
//! prelude, the script, and the exception turned into a [`ScriptError`].

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use rquickjs::context::intrinsic::{Eval, Json, MapSet, Promise, RegExp, RegExpCompiler};
use rquickjs::{
    CatchResultExt, CaughtError, Context, Ctx, Exception, Function, Persistent, Runtime,
    context::EvalOptions,
};
use serde_json::Value;

use crate::{Drawn, Fault, ITEMS, MEMORY, STACK, ScriptError, TIME, Warning};

/// The API the script calls (docs/design/figures.md, Appendix C): one
/// function expression that takes the input and the native emit, puts the
/// API on the global object and returns the check run after the script.
const PRELUDE: &str = concat!(
    "(function (input, emit) {\n'use strict';\n",
    include_str!("prelude/base.js"),
    include_str!("prelude/shapes.js"),
    include_str!("prelude/structure.js"),
    include_str!("prelude/plot.js"),
    include_str!("prelude/checks.js"),
    include_str!("prelude/view.js"),
    "\nreturn finish;\n})"
);

/// The script's file name in a stack trace: [`at`] looks for it.
const SCRIPT: &str = "script";

/// Runs `script` after the prelude, given `data` and `input` as JSON.
pub(crate) fn run(script: &str, data: &str, input: &str) -> Result<Drawn, ScriptError> {
    let internal = |e: rquickjs::Error| {
        ScriptError::new(Fault::Script, (0, 0), format!("the engine failed: {e}"))
    };
    let runtime = Runtime::new().map_err(internal)?;
    runtime.set_memory_limit(MEMORY);
    runtime.set_max_stack_size(STACK);
    let deadline = Instant::now() + TIME;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let rejected = Rc::new(RefCell::new(Rejected::default()));
    let tracker = Rc::clone(&rejected);
    runtime.set_host_promise_rejection_tracker(Some(Box::new(move |_, _, reason, handled| {
        tracker.borrow_mut().saw(reason, handled);
    })));
    let context =
        Context::custom::<(Eval, Json, RegExpCompiler, RegExp, MapSet, Promise)>(&runtime)
            .map_err(internal)?;
    let out = Rc::new(RefCell::new(Drawn::default()));
    let ran = context.with(|ctx| {
        let sink = Rc::clone(&out);
        let emit = Function::new(ctx.clone(), move |ctx: Ctx<'_>, json: String| {
            emitted(&ctx, &sink, &json)
        });
        let finish = (|| {
            let input = ctx.json_parse(input)?;
            input.as_object().map(|o| o.set("data", ctx.json_parse(data)?)).transpose()?;
            let prelude: Function = ctx.eval_with_options(PRELUDE, options("prelude"))?;
            prelude.call::<_, Function>((input, emit?))
        })()
        .catch(&ctx)
        .map_err(|e| failed(&e))?;
        ctx.eval_with_options::<(), _>(script, options(SCRIPT))
            .catch(&ctx)
            .map_err(|e| failed(&e))?;
        Ok(Persistent::save(&ctx, finish))
    });
    // Promise jobs (an async function, a dynamic import) run now, before
    // the checks; none may be left when the runtime goes.
    let jobs = drain(&runtime);
    let ended = (|| {
        let finish = ran?;
        jobs?;
        rejected.take().check()?;
        context.with(|ctx| {
            let finish = finish.restore(&ctx).map_err(internal)?;
            finish.call::<_, ()>(()).catch(&ctx).map_err(|e| failed(&e))
        })
    })();
    let drawn = out.take();
    // Too many shapes is a limit however the script went on: a script
    // that caught the throw still fails.
    match ended {
        _ if drawn.items.len() > ITEMS => Err(full(ended.err())),
        Err(e) => Err(e),
        Ok(()) => Ok(drawn),
    }
}

/// The error for a figure with more than [`ITEMS`] shapes: where the
/// throw that said so stopped the script, if it did.
fn full(stopped: Option<ScriptError>) -> ScriptError {
    let message = format!("{FULL} {ITEMS} shapes");
    match stopped {
        Some(e) if e.message.contains(FULL) => ScriptError { kind: Fault::Limit, message, ..e },
        _ => ScriptError::new(Fault::Limit, (0, 0), message),
    }
}

/// What the throw for one shape too many says, before the number.
const FULL: &str = "a figure holds at most";

/// Runs every pending job. The first one that throws is the run's error;
/// the rest still run, so nothing is left. Past the deadline the interrupt
/// handler stops each job early.
fn drain(runtime: &Runtime) -> Result<(), ScriptError> {
    let mut first = Ok(());
    loop {
        match runtime.execute_pending_job() {
            Ok(true) => {}
            Ok(false) => return first,
            Err(job) => {
                if first.is_ok() {
                    first = Err(job.0.with(|ctx| {
                        match Err::<(), _>(rquickjs::Error::Exception).catch(&ctx) {
                            Err(e) => failed(&e),
                            Ok(()) => ScriptError::new(
                                Fault::Script,
                                (0, 0),
                                "a promise job failed".into(),
                            ),
                        }
                    }));
                }
            }
        }
    }
}

/// The promises rejected with no handler, counted as `QuickJS` reports
/// them: a handler added later takes one back. The first one's reason is
/// the run's error, so a rejection nothing caught fails the draw as a throw
/// would.
#[derive(Default)]
struct Rejected {
    unhandled: usize,
    first: Option<ScriptError>,
}

impl Rejected {
    fn saw(&mut self, reason: rquickjs::Value<'_>, handled: bool) {
        if handled {
            self.unhandled = self.unhandled.saturating_sub(1);
            if self.unhandled == 0 {
                self.first = None;
            }
            return;
        }
        self.unhandled += 1;
        if self.first.is_none() {
            let caught = match reason.clone().into_exception() {
                Some(x) => CaughtError::Exception(x),
                None => CaughtError::Value(reason),
            };
            let mut e = failed(&caught);
            e.message = format!("a promise was rejected and nothing caught it: {}", e.message);
            self.first = Some(e);
        }
    }

    fn check(self) -> Result<(), ScriptError> {
        match self.first {
            Some(e) if self.unhandled > 0 => Err(e),
            _ => Ok(()),
        }
    }
}

fn options(filename: &str) -> EvalOptions {
    let mut o = EvalOptions::default();
    o.global = true;
    o.strict = false;
    o.filename = Some(filename.to_owned());
    o
}

/// Keeps one emitted object: a warning (`{"warning": message, "line",
/// "col"}`) or a shape. The shape after [`ITEMS`] is kept, so the run
/// knows, and throws.
fn emitted(ctx: &Ctx<'_>, out: &RefCell<Drawn>, json: &str) -> rquickjs::Result<()> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Err(rquickjs::Error::new_from_js("string", "a shape as JSON"));
    };
    let mut out = out.borrow_mut();
    if let Some(message) = value.get("warning").and_then(Value::as_str) {
        let number = |k| {
            value.get(k).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).unwrap_or(0)
        };
        let warning =
            Warning { line: number("line"), col: number("col"), message: message.to_owned() };
        out.warnings.push(warning);
        return Ok(());
    }
    out.items.push(value);
    if out.items.len() > ITEMS {
        return Err(Exception::throw_message(ctx, &format!("{FULL} {ITEMS} shapes")));
    }
    Ok(())
}

/// The error a caught exception makes: its message in the script's words,
/// at the first place in the script its stack names.
fn failed(e: &CaughtError<'_>) -> ScriptError {
    let (name, message, stack) = match e {
        CaughtError::Exception(x) => {
            let get = |k: &str| x.get::<_, Option<String>>(k).ok().flatten().unwrap_or_default();
            (get("name"), x.message().unwrap_or_default(), x.stack().unwrap_or_default())
        }
        CaughtError::Value(v) if v.is_null() => {
            (String::new(), "out of memory".to_owned(), String::new())
        }
        CaughtError::Value(v) => {
            let shown = v.as_string().and_then(|s| s.to_string().ok());
            (
                String::new(),
                shown.unwrap_or_else(|| format!("threw a {}", v.type_name())),
                String::new(),
            )
        }
        CaughtError::Error(e) => (String::new(), e.to_string(), String::new()),
    };
    let place = at(&stack).unwrap_or((0, 0));
    let (kind, mut said) = said(&name, &message);
    if message == "interrupted" && place.0 != 0 {
        said.push_str(STOPPED);
    }
    ScriptError { stack: frames(&stack), ..ScriptError::new(kind, place, said) }
}

/// Where a run the clock stopped was. `QuickJS` checks the clock at a call
/// and at a jump back: at a call the stack names the call; at a jump back
/// it names the last position recorded before the jump's target, the head
/// of the loop, so a line before the loop (the first of its function when
/// nothing came before), which moves with whatever ran before. Either way
/// the loop is at that line or after it, and the message says no more.
const STOPPED: &str = ", stopped at this line or in a loop after it";

/// The fault and the message for an exception named `name`: a limit says
/// which, the prelude's own errors stand as they are, anything else keeps
/// its name.
fn said(name: &str, message: &str) -> (Fault, String) {
    let limit = |s: String| (Fault::Limit, s);
    if message == "interrupted" {
        return limit(format!("the script ran longer than {} s", TIME.as_secs()));
    }
    if message.starts_with("out of memory") {
        return limit(format!("the script used more than {} MB", MEMORY / (1024 * 1024)));
    }
    if message.contains("stack overflow") || message.contains("call stack size exceeded") {
        return limit(format!(
            "the script recursed too deep (more than {} KB of stack)",
            STACK / 1024
        ));
    }
    match name {
        "" | "DrawError" => (Fault::Script, message.to_owned()),
        _ => (Fault::Script, format!("{name}: {message}")),
    }
}

/// The frames of a stack trace that are in the script, innermost first,
/// as `name (script:L:C)`; the script's top level is `top level`.
fn frames(stack: &str) -> Vec<String> {
    let key = format!("({SCRIPT}:");
    stack
        .lines()
        .filter_map(|l| l.trim().strip_prefix("at "))
        .filter(|f| f.contains(&key))
        .map(|f| match f.split_once(" (") {
            Some(("<eval>" | "<anonymous>", place)) => format!("top level ({place}"),
            _ => f.to_owned(),
        })
        .collect()
}

/// The first `script:LINE:COL` in a stack trace.
fn at(stack: &str) -> Option<(u32, u32)> {
    let key = format!("{SCRIPT}:");
    stack.match_indices(&key).find_map(|(i, _)| {
        let mut numbers = stack[i + key.len()..].splitn(3, ':');
        let line = numbers.next()?.parse().ok()?;
        let col: String = numbers.next()?.chars().take_while(char::is_ascii_digit).collect();
        Some((line, col.parse().ok()?))
    })
}
