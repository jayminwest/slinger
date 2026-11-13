//! A sling over a PDF through the real socket (docs/design/pdf.md):
//! `sling_backdrops` names the areas under the new ink, and `sling_send`
//! reads the files rendered for them, draws the ink over them and ends the
//! ink's line with the words; a file that is not there or not right stores
//! nothing.

use serde_json::json;
use slinger_control::{Backdrop, CanvasOps, Client, SlingAsk};
use tempfile::TempDir;

use super::Daemon;
use super::session::open_with_agent;

fn apply(client: &Client, item: &serde_json::Value) {
    let ops = json!({ "op": "add", "item": item });
    let ops = CanvasOps { canvas: "notes".into(), by: "user".into(), ops, once: None };
    client.canvas_apply(ops).unwrap();
}

#[test]
fn a_sling_over_a_pdf_takes_the_page_and_its_words() {
    let (data, projects, files) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    let hash = format!("sha256:{}", "9f".repeat(32));
    apply(
        &client,
        &json!({ "page": "p1", "kind": "pdf", "x": 0, "y": 0, "w": 16000,
        "h": 20706, "hash": hash, "pdfpage": 1, "pagesize": [6120, 7920], "name": "paper.pdf" }),
    );
    assert_eq!(client.sling_backdrops("notes", None).unwrap(), [], "no ink yet");
    apply(
        &client,
        &json!({ "page": "p1", "kind": "stroke", "color": "#ff0000", "width": 30,
        "pts": [1000, 1000, 400, 0] }),
    );
    let [area] = client.sling_backdrops("notes", Some("p1")).unwrap().try_into().unwrap();
    assert_eq!((area.item.as_str(), area.hash.as_str(), area.pdfpage), ("i1", &*hash, 1));
    assert_eq!((area.pagesize, area.crop, area.frame), ([6120, 7920], None, [0, 0, 16000, 20706]));
    assert_eq!((area.area, area.width, area.height), ([825, 825, 750, 350], 27, 13));
    assert_eq!(client.sling_backdrops("gone", None).unwrap_err().code(), "not_found");

    let image = files.path().join("1.rgb");
    let words = files.path().join("1.json");
    let backdrop = Backdrop { item: area.item, area: area.area, image, words: Some(words) };
    let ask = SlingAsk { canvas: "notes".into(), backdrops: vec![backdrop], ..SlingAsk::default() };
    assert_eq!(client.sling_send(ask.clone()).unwrap_err().code(), "not_found", "no files yet");
    std::fs::write(&ask.backdrops[0].image, vec![0; 27 * 13 * 3 - 1]).unwrap();
    std::fs::write(ask.backdrops[0].words.as_ref().unwrap(), "[]").unwrap();
    assert_eq!(client.sling_send(ask.clone()).unwrap_err().code(), "invalid", "a byte short");
    std::fs::write(&ask.backdrops[0].image, vec![0; 27 * 13 * 3]).unwrap();
    let said = json!([{ "text": "circled", "x": 1000, "y": 950, "w": 400, "h": 100 }]);
    std::fs::write(ask.backdrops[0].words.as_ref().unwrap(), said.to_string()).unwrap();
    let slung = client.sling_send(ask).unwrap();
    assert_eq!(slung.strokes, 1, "nothing was stored before");
    let text = files.path().join("packet.txt");
    client.blob_get(&slung.packet, &text).unwrap();
    let text = std::fs::read_to_string(text).unwrap();
    assert!(text.contains("over=d1 img=c2.png \"circled\"\n"), "{text}");
    assert_eq!(client.sling_backdrops("notes", None).unwrap(), [], "nothing new since");
    daemon.stop();
}
