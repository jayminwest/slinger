//! What an agent asks a canvas for beyond the packet: `canvas_crop` (images
//! written as files), `canvas_strokes` and `canvas_find`.

use std::fs;
use std::io;

use slinger_control::{
    CanvasCropAsk, CanvasFindAsk, CanvasFound, CanvasImage, CanvasImages, CanvasStrokes,
    CanvasStrokesAsk, RemoteError,
};

use super::{absolute, drawn, unusable};
use crate::store::Store;

/// Writes a crop's images: `path` names the one image, and tiles go beside
/// it as `<stem>-<n>.png`. Bytes travel as files, as a blob's do.
pub(super) fn crop(
    store: &Store,
    CanvasCropAsk { canvas, of, path }: CanvasCropAsk,
) -> Result<CanvasImages, RemoteError> {
    let (canvas, of) = (canvas.as_str(), of.as_str());
    let path = absolute(&path)?;
    let (Some(dir), Some(name)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else {
        return Err(unusable(path, &io::Error::other("it names no file")));
    };
    let images = store.canvases().crop(canvas, of, name).map_err(|e| drawn(&e))?;
    fs::create_dir_all(dir).map_err(|e| unusable(dir, &e))?;
    let images = images
        .into_iter()
        .map(|image| {
            let path = dir.join(&image.name);
            fs::write(&path, &image.png).map_err(|e| unusable(&path, &e))?;
            let (x, y, width, height) = (image.x, image.y, image.width, image.height);
            Ok(CanvasImage { path, x, y, width, height })
        })
        .collect::<Result<_, RemoteError>>()?;
    Ok(CanvasImages { images })
}

/// The strokes `item` names, with their points.
pub(super) fn strokes(
    store: &Store,
    CanvasStrokesAsk { canvas, item }: CanvasStrokesAsk,
) -> Result<CanvasStrokes, RemoteError> {
    let strokes = store.canvases().strokes(&canvas, &item).map_err(|e| drawn(&e))?;
    Ok(CanvasStrokes { strokes })
}

/// The text items that hold `text`, on every page.
pub(super) fn find(
    store: &Store,
    CanvasFindAsk { canvas, text }: CanvasFindAsk,
) -> Result<CanvasFound, RemoteError> {
    let found = store.canvases().find(&canvas, &text).map_err(|e| drawn(&e))?;
    Ok(CanvasFound { found })
}
