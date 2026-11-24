//! One call of each `Client` method, with every optional field set, and
//! again with none where a verb has optional fields: the golden request
//! lines pin both the field order and what is left out.

use std::path::Path;
use std::time::Duration;

use serde_json::json;
use slinger_control::{
    Backdrop, CanvasDraw, CanvasEdit, CanvasFold, CanvasOps, CanvasPut, CanvasStep, Client,
    EventFilter, FeedbackNote, FoldAsk, NewEvent, NewTags, PacketAsk, PaneKind, PaneView,
    PaneViewed, Place, Profile, Put, RemoteError, SlingAsk, Touched, UiOpen,
};

type Call = fn(&Client) -> Result<(), RemoteError>;

fn event(tags: NewTags, payload: serde_json::Value) -> NewEvent {
    NewEvent { kind: "agent_done".into(), source: "scout".into(), tags, payload }
}

fn ops(once: Option<&str>) -> CanvasOps {
    let ops = json!([{"op": "add", "kind": "text", "text": "hi"}]);
    CanvasOps { canvas: "main".into(), by: "user".into(), ops, once: once.map(str::to_string) }
}

fn step() -> CanvasStep {
    CanvasStep { canvas: "main".into(), by: "agent:scout".into() }
}

fn view(id: u32, page: Option<&str>) -> PaneViewed {
    let page = page.map(str::to_string);
    PaneViewed { id, view: PaneView { canvas: "main".into(), page, x: 1.5, y: -2.0, zoom: 1.25 } }
}

fn fold(once: Option<&str>) -> CanvasFold {
    let once = once.map(str::to_string);
    CanvasFold { canvas: "main".into(), item: "b4".into(), by: "user".into(), once }
}

fn put(what: Put) -> CanvasPut {
    let (canvas, agent, beside) = ("main".into(), "scout".into(), "c12".into());
    CanvasPut { canvas, agent, beside, what, color: None, at: None, text_size: None, inside: None }
}

fn draw(place: Place) -> CanvasDraw {
    CanvasDraw {
        canvas: "main".into(),
        by: "agent:scout".into(),
        name: "worksheet".into(),
        path: "/tmp/w.js".into(),
        data: None,
        place,
        page: None,
        dry_run: false,
        png: None,
    }
}

const TS: &str = "2026-11-11T14:02:09.120-08:00";

/// Every call, in the order of `fixtures/requests.jsonl`.
pub const CALLS: &[Call] = &[
    |c| {
        let tags = NewTags {
            workspace: Some("w".into()),
            agent: Some("scout".into()),
            canvas: Some("main".into()),
        };
        c.record_append(event(tags, json!({"summary": "wrote the plan"}))).map(drop)
    },
    |c| c.record_append(event(NewTags::default(), json!({}))).map(drop),
    |c| {
        let filter = EventFilter {
            kind: Some("agent_done".into()),
            machine: Some("my-laptop".into()),
            workspace: Some("w".into()),
            agent: Some("scout".into()),
            canvas: Some("main".into()),
            after: Some(3),
        };
        c.record_list(filter).map(drop)
    },
    |c| c.record_list(EventFilter::default()).map(drop),
    |c| c.blob_put(Path::new("/tmp/a.bin")).map(drop),
    |c| c.blob_get("sha256:ab", Path::new("/tmp/b.bin")).map(drop),
    |c| c.ui_put("chart", Path::new("/tmp/chart.html"), "scout").map(drop),
    |c| c.ui_list(None).map(drop),
    |c| c.ui_list(Some("chart")).map(drop),
    |c| c.ui_export("chart@2", Path::new("/tmp/out")).map(drop),
    |c| {
        let open = UiOpen {
            ui: "chart".into(),
            workspace: "w".into(),
            agent: "scout".into(),
            host: "pane".into(),
            data: json!({"rows": [1, 2]}),
            beside: Some(2),
            below: true,
            canvas: Some("main".into()),
            item: Some("i4".into()),
            size: Some([3200, 2400]),
        };
        c.ui_open(open).map(drop)
    },
    |c| {
        let open = UiOpen {
            ui: "chart".into(),
            agent: "scout".into(),
            host: "canvas".into(),
            ..UiOpen::default()
        };
        c.ui_open(open).map(drop)
    },
    |c| c.ui_shown("o1").map(drop),
    |c| c.ui_reply("o1", json!({"pick": 2})).map(drop),
    |c| c.canvas_create("main"),
    |c| c.canvas_create_with("main", Path::new("/tmp/ops.json"), Some("agent:scout")),
    |c| c.canvas_create_with("main", Path::new("/tmp/ops.json"), None),
    |c| c.canvas_list().map(drop),
    |c| c.canvas_show("main", Some("p2")).map(drop),
    |c| c.canvas_show("main", None).map(drop),
    |c| c.canvas_apply(ops(Some("k1"))).map(drop),
    |c| c.canvas_apply(ops(None)).map(drop),
    |c| c.canvas_undo(step()).map(drop),
    |c| c.canvas_redo(step()).map(drop),
    |c| c.canvas_import(Path::new("/tmp/b.tldr"), Some("board")).map(drop),
    |c| c.canvas_import(Path::new("/tmp/b.tldr"), None).map(drop),
    // The app reads only the new canvas's id (it once had a method of its own).
    |c| c.canvas_import(Path::new("/tmp/b.tldr"), Some("board")).map(|made| drop(made.canvas)),
    |c| c.canvas_changes("main", 6, Duration::from_millis(100)).map(drop),
    |c| c.workspace_open(Some(Path::new("/p/w")), Some("w")).map(drop),
    |c| c.workspace_open(None, Some("w")).map(drop),
    |c| c.workspace_list().map(drop),
    |c| c.workspace_show("w").map(drop),
    |c| c.workspace_close("/p/w").map(drop),
    |c| c.pane_list("w").map(drop),
    |c| c.pane_open("w", PaneKind::Ui, "chart", Some(2), true).map(drop),
    |c| c.pane_open("w", PaneKind::Canvas, "main", None, false).map(drop),
    |c| c.pane_close("w", 2).map(drop),
    |c| c.pane_resize("w", 1, 0.25).map(drop),
    |c| c.pane_view("w", &[view(1, Some("p2")), view(3, None)], Some(3)).map(drop),
    |c| c.pane_view("w", &[], None).map(drop),
    |c| {
        let ask = PacketAsk {
            canvas: "main".into(),
            page: Some("p2".into()),
            inside: Some("b4".into()),
            after: Some(3),
            select: vec!["c1".into(), "t2".into()],
            out: Some("/tmp/p".into()),
        };
        c.canvas_read(ask).map(drop)
    },
    |c| c.canvas_read(PacketAsk { canvas: "main".into(), ..PacketAsk::default() }).map(drop),
    |c| c.canvas_crop("main", "p1:0,0,40,20", Path::new("/tmp/c.png")).map(drop),
    |c| c.canvas_strokes("main", "c4").map(drop),
    |c| c.canvas_find("main", "plan B").map(drop),
    |c| c.canvas_fold(&FoldAsk::CanvasFold(fold(Some("k2")))).map(drop),
    |c| c.canvas_fold(&FoldAsk::CanvasUnfold(fold(None))).map(drop),
    |c| c.canvas_fold_step(&FoldAsk::CanvasFold(fold(None))).map(drop),
    |c| c.canvas_fold_step(&FoldAsk::CanvasUnfold(fold(Some("k3")))).map(drop),
    |c| c.session_list().map(drop),
    |c| c.session_show("scout").map(drop),
    |c| c.status_set("scout", "done", Some("wrote it")).map(drop),
    |c| c.status_set("scout", "working", None).map(drop),
    |c| c.canvas_put("main", "scout", "c12", Put::Text("hi".into())).map(drop),
    |c| c.canvas_put("main", "scout", "c12", Put::Box([200, 100])).map(drop),
    |c| c.canvas_put("main", "scout", "c12", Put::Ink(vec![0, 0, 50, 10])).map(drop),
    |c| c.canvas_put("main", "scout", "c12", Put::Arrow(["c1".into(), "t2".into()])).map(drop),
    |c| {
        let mut ask = put(Put::Text("hi".into()));
        (ask.color, ask.at) = (Some("#ffd400".into()), Some([10, 20]));
        (ask.text_size, ask.inside) = (Some(480), Some("b4".into()));
        c.canvas_put_as(ask).map(drop)
    },
    |c| {
        let edit = CanvasEdit {
            canvas: "main".into(),
            agent: "scout".into(),
            item: "i2".into(),
            text: Some("hi".into()),
            text_size: Some(480),
            size: Some([300, 200]),
            to: Some([10, 20]),
        };
        c.canvas_edit(edit).map(drop)
    },
    |c| {
        let (canvas, agent, item) = ("main".into(), "scout".into(), "i2".into());
        c.canvas_edit(CanvasEdit { canvas, agent, item, ..CanvasEdit::default() }).map(drop)
    },
    |c| c.canvas_remove("main", "scout", "i2").map(drop),
    |c| c.session_start("w").map(drop),
    |c| c.session_start_fresh("w").map(drop),
    |c| c.session_start_with("w", Some("dev"), true).map(drop),
    |c| c.session_start_with("w", Some("dev"), false).map(drop),
    |c| c.session_stop("w").map(drop),
    |c| c.session_send("w", "hi").map(drop),
    |c| c.session_listen("w", Duration::from_millis(250)).map(drop),
    |c| c.session_listen_in("w", Duration::ZERO, Some(7)).map(drop),
    |c| c.sling_backdrops("main", Some("p1"), Some("b4")).map(drop),
    |c| c.sling_backdrops("main", None, None).map(drop),
    |c| {
        let ask = SlingAsk {
            canvas: "main".into(),
            page: Some("p1".into()),
            inside: Some("b4".into()),
            select: vec!["c4".into(), "t9".into()],
            prompt: "why?".into(),
            profile: Some("dev".into()),
            backdrops: vec![Backdrop {
                item: "i3".into(),
                area: [0, 10, 200, 300],
                image: "/tmp/1.rgb".into(),
                words: Some("/tmp/1.json".into()),
            }],
        };
        c.sling_send(ask).map(drop)
    },
    |c| c.sling_send(SlingAsk { canvas: "main".into(), ..SlingAsk::default() }).map(drop),
    |c| c.profile_list("w").map(drop),
    |c| {
        let profile = Profile {
            name: "dev".into(),
            dir: Some("/p/w/dev".into()),
            tools: vec!["Bash(cargo:*)".into(), "Read".into()],
        };
        c.profile_set("w", profile).map(drop)
    },
    |c| c.profile_set("w", Profile { name: "plain".into(), ..Profile::default() }).map(drop),
    |c| c.profile_remove("w", "dev").map(drop),
    |c| {
        let note = FeedbackNote {
            text: "the lasso is slow".into(),
            workspace: Some("w".into()),
            canvas: Some("main".into()),
            page: Some("Page 1".into()),
            tool: Some("lasso".into()),
            selected: Some(3),
        };
        c.feedback_add(note).map(drop)
    },
    |c| c.feedback_add(FeedbackNote { text: "hi".into(), ..FeedbackNote::default() }).map(drop),
    |c| c.feedback_list(true, Some(5)).map(drop),
    |c| c.feedback_list(false, None).map(drop),
    |c| c.feedback_resolve(TS, Some("fixed".into())).map(drop),
    |c| c.feedback_resolve(TS, None).map(drop),
    |c| c.feedback_path().map(drop),
    |c| c.daemon_info().map(drop),
    |c| c.daemon_stop().map(drop),
    |c| c.file_touched("scout", Touched::Path(Path::new("/p/w/a.rs"))).map(drop),
    |c| c.file_touched("scout", Touched::Hook(r#"{"tool_name":"Write"}"#)).map(drop),
    |c| {
        let draw = CanvasDraw {
            data: Some("/tmp/d.json".into()),
            page: Some("p2".into()),
            dry_run: true,
            png: Some("/tmp/w.png".into()),
            ..draw(Place::At([10, -20]))
        };
        c.canvas_draw(draw).map(drop)
    },
    |c| c.canvas_draw(draw(Place::Beside("c12".into()))).map(drop),
];
