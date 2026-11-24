//! The `QuickJS` glue: a fresh runtime per run with the limits set, the
//! prelude, the script, and the exception turned into a [`ScriptError`].

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use rquickjs::context::intrinsic::{Eval, Json, MapSet, Promise, RegExp, RegExpCompiler};
use rquickjs::{
    CatchResultExt, CaughtError, Context, Function, Persistent, Runtime, context::EvalOptions,
};
use serde_json::Value;

use crate::{Drawn, MEMORY, STACK, ScriptError, TIME, Warning};

/// The API the script calls (docs/design/figures.md, Appendix A): one
/// function expression that takes the input and the native emit, puts the
/// API on the global object and returns the check run after the script.
const PRELUDE: &str = concat!(
    "(function (input, emit) {\n'use strict';\n",
    include_str!("prelude/base.js"),
    include_str!("prelude/shapes.js"),
    include_str!("prelude/structure.js"),
    include_str!("prelude/plot.js"),
    include_str!("prelude/checks.js"),
    "\nreturn finish;\n})"
);

/// The script's file name in a stack trace: [`at`] looks for it.
const SCRIPT: &str = "script";

/// Runs `script` after the prelude, given `data` and `input` as JSON.
pub(crate) fn run(script: &str, data: &str, input: &str) -> Result<Drawn, ScriptError> {
    let internal = |e: rquickjs::Error| ScriptError {
        line: 0,
        col: 0,
        message: format!("the engine failed: {e}"),
    };
    let runtime = Runtime::new().map_err(internal)?;
    runtime.set_memory_limit(MEMORY);
    runtime.set_max_stack_size(STACK);
    let deadline = Instant::now() + TIME;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context =
        Context::custom::<(Eval, Json, RegExpCompiler, RegExp, MapSet, Promise)>(&runtime)
            .map_err(internal)?;
    let out = Rc::new(RefCell::new(Drawn::default()));
    let ran = context.with(|ctx| {
        let sink = Rc::clone(&out);
        let emit = Function::new(ctx.clone(), move |json: String| emitted(&sink, &json));
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
    let finish = ran?;
    jobs?;
    context.with(|ctx| {
        let finish = finish.restore(&ctx).map_err(internal)?;
        finish.call::<_, ()>(()).catch(&ctx).map_err(|e| failed(&e))
    })?;
    Ok(out.take())
}

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
                            Ok(()) => ScriptError {
                                line: 0,
                                col: 0,
                                message: "a promise job failed".into(),
                            },
                        }
                    }));
                }
            }
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
/// "col"}`) or a shape.
fn emitted(out: &RefCell<Drawn>, json: &str) -> rquickjs::Result<()> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Err(rquickjs::Error::new_from_js("string", "a shape as JSON"));
    };
    let mut out = out.borrow_mut();
    match value.get("warning").and_then(Value::as_str) {
        Some(message) => {
            let number = |k| {
                value
                    .get(k)
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(0)
            };
            let warning =
                Warning { line: number("line"), col: number("col"), message: message.to_owned() };
            out.warnings.push(warning);
        }
        None => out.items.push(value),
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
    let (line, col) = at(&stack).unwrap_or((0, 0));
    ScriptError { line, col, message: said(&name, &message) }
}

/// The message for an exception named `name`: a limit says which, the
/// prelude's own errors stand as they are, anything else keeps its name.
fn said(name: &str, message: &str) -> String {
    if message == "interrupted" {
        return format!("the script ran longer than {} s", TIME.as_secs());
    }
    if message.starts_with("out of memory") {
        return format!("the script used more than {} MB", MEMORY / (1024 * 1024));
    }
    if message.contains("stack overflow") || message.contains("call stack size exceeded") {
        return format!("the script recursed too deep (more than {} KB of stack)", STACK / 1024);
    }
    match name {
        "" | "DrawError" => message.to_owned(),
        _ => format!("{name}: {message}"),
    }
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
