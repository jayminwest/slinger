//! What the window does with a command (`run`, and the selection's commands
//! in `selection`), a tap on a button that does more (`tap`) or a cut, copy
//! or paste no text field took (`edit`, and `can_edit` to grey it), a PDF to
//! open (`open_pdf`), and what it
//! shows (`view`), and where its panes are: the canvas pane under a point
//! (`pane_at`), the focused pane (`focus`) and whether a press grabs the
//! selection (`grabs`). Split from `main.rs`, which
//! routes the events here.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::Event;
use crate::backend::pdf::Pages;
use crate::backend::{self, Settings};
use crate::canvases::Canvases;
use crate::commands::{self, Command};
use crate::layout::{PaneId, Point, Rect};
use crate::panes::{self, Panes};
use crate::scene::ink::Pen;
use crate::scene::pages::menu::List;
use crate::scene::paste::{self, Clip, Edit};
use crate::scene::sidebar::Row;
use crate::scene::sidebar::Sidebar;
use crate::scene::sling::{CANCEL, Profile, SEND};
use crate::scene::taps::{Tap, Target};
use crate::scene::tools::{Kind, Tools};
use crate::scene::{self, Label, Scene, cursor, status, title};
use crate::shared::Shared;

pub mod button;
mod facts;
mod items;
pub mod launched;
mod workspace;
pub use facts::facts;
pub use items::keyboard;
pub use workspace::{close_workspace, open_workspace};

/// Runs `command` (not `quit` or `reload-config`) on the focused pane of
/// `shared`: a picker opens its pop-up there; a pane command, and an
/// agent command for the workspace `sidebar` shows, go to the daemon on
/// `socket`. False for a selection command with nothing selected:
/// the key goes on (a delete in a UI's form).
pub fn run(
    command: Command,
    tools: &mut Tools,
    canvases: &mut Canvases,
    sidebar: &mut Sidebar,
    shared: &mut Shared,
    socket: &Path,
) -> bool {
    let area = sidebar.panes(shared.size.rect());
    match command {
        c if Kind::opened_by(c).is_some_and(|kind| {
            tools.open(kind, focused(&shared.panes, area));
            true
        }) => {}
        c if !selection(canvases, shared, tools, area, c) => return false,
        Command::ToggleSidebar => sidebar.set_open(!sidebar.is_open()),
        Command::DismissNotice => sidebar.dismiss(),
        Command::StepInBeside => return items::beside(canvases, shared, area, socket),
        Command::SplitRight | Command::SplitDown => {
            items::split(command, canvases, shared, area, socket);
        }
        Command::StartAgent | Command::StopAgent | Command::RestartAgent => {
            agent(command, socket, sidebar, tools);
        }
        c if canvases.command(&shared.panes, focus(&shared.panes, area), c) || tools.command(c) => {
        }
        c => shared.command(c, area, socket),
    }
    true
}

/// A tap on `target` (a press already focused its pane in `shared`): the
/// page name opens the page menu over it, a page menu's page turns the
/// focused pane to it, the footer's agent opens its profiles over it and
/// one of them slings under it, a workspace in `sidebar` shows its panes (that
/// workspace's name, for the watch to follow), and the sling's pill does
/// what a tap there does; the open entry's "✓" and "✕" do what return and
/// escape do in it, and a "✓" at a recent note's end under the feedback
/// entry resolves the note. A command's tap does nothing here.
pub fn tap(
    (frame, tap): Target,
    canvases: &mut Canvases,
    shared: &mut Shared,
    sidebar: &mut Sidebar,
) -> Option<String> {
    let pane = shared.panes.focus();
    match tap {
        Tap::Pages => {
            if let Some(pages) = canvases.page_names(&shared.panes, pane) {
                shared.menu.open(pane, frame, pages);
            }
        }
        Tap::Page(index) => canvases.turn_to(&shared.panes, pane, index),
        Tap::Profiles => {
            if let Some(profiles) = canvases.profile_names(&shared.panes, pane) {
                shared.menu.open_profiles(pane, frame, profiles);
            }
        }
        Tap::Profile(index) => {
            let area = sidebar.panes(shared.size.rect());
            canvases.sling_under(&shared.panes, area, index);
        }
        Tap::Workspace(index) => return sidebar.pick(index),
        Tap::Slung => {
            let area = sidebar.panes(shared.size.rect());
            canvases.tap_slung(&shared.panes, area);
        }
        Tap::Send => _ = canvases.sling_key(SEND),
        Tap::Cancel => _ = canvases.sling_key(CANCEL),
        Tap::Resolve(index) => _ = canvases.resolve_feedback(index),
        Tap::Waive => sidebar.waive(),
        Tap::Run(_) | Tap::Close(_) => {}
    }
    None
}

/// Runs `start-agent`, `stop-agent` or `restart-agent` on the agent of
/// the workspace `sidebar` shows,
/// through the daemon on `socket`: the flash in `tools` names the agent,
/// or says why it cannot.
pub fn agent(command: Command, socket: &Path, sidebar: &Sidebar, tools: &mut Tools) {
    let Some(dir) = sidebar.shown_dir() else { return };
    let daemon = slinger_control::Client::new(socket);
    let done = match command {
        Command::StopAgent => backend::stop_agent(&daemon, &dir),
        c => backend::start_agent(&daemon, &dir, c == Command::RestartAgent),
    };
    let said = match done {
        Ok(agent) => format!("{} · {agent}", command.title()),
        Err(problem) => problem.message,
    };
    tools.flash().say(said, Instant::now());
}

/// What `show-canvas-file`, `show-workspace-folder` or
/// `show-feedback-file` (`command`) shows in the Finder: the focused pane's
/// canvas file, the folder of the workspace `sidebar` shows, or the
/// daemon's feedback file; else what the flash says.
pub fn finder(
    command: Command,
    shared: &Shared,
    canvases: &Canvases,
    sidebar: &Sidebar,
) -> Result<String, String> {
    if command == Command::ShowWorkspaceFolder {
        let dir = sidebar.shown_dir().ok_or("No workspace is open")?;
        return Ok(dir.to_string_lossy().into_owned());
    }
    if command == Command::ShowFeedbackFile {
        return canvases.feedback_file();
    }
    match canvases.file(&shared.panes, shared.panes.focus()) {
        None => Err(title::NO_CANVAS.to_string()),
        Some((canvas, None)) => Err(format!("slingerd has not said where {canvas} is saved")),
        Some((_, Some(file))) => Ok(file.to_string()),
    }
}

/// Saves the agent UI the focused pane of `shared` shows, the version it
/// shows, in the folder `dir`, named as `ui_export` names it
/// (`<name>.v<version>.html`), from the bytes the pane already has: the file
/// written, for the Finder to show; else what the flash says.
pub fn export_ui(shared: &Shared, dir: &Path) -> Result<String, String> {
    let Some(ui) = shown_ui(shared) else {
        return Err("The focused pane shows no agent UI".into());
    };
    let file = dir.join(format!("{}.v{}.html", ui.code.name, ui.code.version));
    std::fs::write(&file, &ui.code.html).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}

/// The agent UI the focused pane of `shared` shows, once it has arrived.
pub fn shown_ui(shared: &Shared) -> Option<&crate::ui::Page> {
    let pane = shared.panes.focused();
    let ui = shared.uis.get(&pane.name).filter(|_| pane.kind == panes::Kind::Ui);
    ui.and_then(|ui| ui.as_deref().ok())
}

/// Opens the PDF or tldraw `file` as a canvas beside the focused pane of
/// `shared`, a PDF's pages read by `sizes` ([`Canvases::open_pdf`]):
/// `open-pdf`'s file, or the first PDF or tldraw file of files dropped on
/// the window, which focuses the pane under `at` (a point in `area`)
/// first. False when no file is either.
pub fn open_pdf(
    files: &[PathBuf],
    at: Option<Point>,
    shared: &mut Shared,
    canvases: &mut Canvases,
    area: Rect,
    sizes: Pages,
) -> bool {
    let pdf = |f: &&PathBuf| {
        f.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) || backend::pdf::is_tldraw(f)
    };
    let Some(file) = files.iter().find(pdf) else { return false };
    if let Some(at) = at {
        shared.panes.focus_at(area, at);
    }
    canvases.open_pdf(&shared.panes, shared.workspace.as_deref(), file.clone(), sizes);
    true
}

/// Runs a selection command (`sling`, `dictate`: the entry; `sling-now`) when the focused pane
/// of `shared` laid in `area` shows its canvas; false when it does not. `leave-feedback` opens its entry
/// there, with the pen's tool in `tools`, listing the open notes; `show-all-feedback` the same, listing
/// the resolved ones too. An item command is [`items::run`]'s. Any other command goes on (true).
pub fn selection(
    canvases: &mut Canvases,
    shared: &Shared,
    tools: &Tools,
    area: Rect,
    command: Command,
) -> bool {
    let panes = &shared.panes;
    match command {
        Command::LeaveFeedback | Command::ShowAllFeedback => {
            let (at, workspace) = (focus(panes, area), shared.workspace.as_deref());
            canvases.list_all_feedback(command == Command::ShowAllFeedback);
            canvases.open_feedback(panes, at, workspace, tools.tool());
            true
        }
        Command::DeleteSelection => items::delete(shared, canvases),
        Command::ClearSelection => items::clear(shared, canvases),
        Command::Sling | Command::Dictate => canvases.sling_command(command, panes, area),
        Command::SlingNow => canvases.sling_now_command(panes, area, Profile::Running),
        Command::NewText => {
            let under = tools.pointer().and_then(|at| pane_at(panes, area, at));
            let pane = under.unwrap_or_else(|| focus(panes, area));
            canvases.new_text(panes, pane, tools.pointer(), tools.colour())
        }
        c => items::run(c, shared, canvases, area),
    }
}

/// Whether `edit` would do anything in `shared` and `canvases`, so its
/// Edit menu item is not grey: under the key overlay or the page menu,
/// nothing; the open palette's search takes pasted text; else the
/// canvases say ([`crate::canvases`]' `clip`).
pub fn can_edit(edit: &Edit, shared: &Shared, canvases: &Canvases) -> bool {
    let palette = shared.palette.is_open();
    !shared.overlay.is_open()
        && !shared.menu.is_open()
        && match edit {
            Edit::Cut | Edit::Copy => !palette && canvases.can_copy(&shared.panes),
            Edit::Paste(clip) if palette => clip.text.is_some(),
            Edit::Paste(clip) => canvases.can_paste(clip, &shared.panes),
        }
}

/// What a cut, a copy or a paste no text field took did.
pub enum Edited {
    /// Nothing: nothing took it.
    No,
    /// Something took it, or a paste flashed why not: draw again, and put
    /// the clip on the clipboard (a copy's or a cut's).
    Drawn(Option<Clip>),
}

/// Does `edit` where [`can_edit`] says, on the focused pane of `shared`
/// laid in `area`; pasted text that makes a new text item has `tools`'
/// colour. A paste nothing took flashes why ([`unpasted`]).
pub fn edit(
    edit: &Edit,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &Tools,
    area: Rect,
) -> Edited {
    let took = can_edit(edit, shared, canvases)
        && match edit {
            Edit::Cut | Edit::Copy => {
                let clip = canvases.copy(&shared.panes, *edit == Edit::Cut);
                return clip.map_or(Edited::No, |clip| Edited::Drawn(Some(clip)));
            }
            Edit::Paste(Clip { text: Some(text), .. }) if shared.palette.is_open() => {
                shared.palette.pasted(text)
            }
            Edit::Paste(clip) => canvases.paste(clip, &shared.panes, area, tools.colour()),
        };
    match edit {
        _ if took => Edited::Drawn(None),
        Edit::Paste(clip) => {
            unpasted(Some(clip), shared, canvases, tools);
            Edited::Drawn(None)
        }
        Edit::Cut | Edit::Copy => Edited::No,
    }
}

/// Flashes in `tools` why a paste of `clip` went nowhere ([`paste::unpasted`]):
/// `None` when only its key came, its Edit menu item greyed.
pub fn unpasted(clip: Option<&Clip>, shared: &Shared, canvases: &Canvases, tools: &Tools) {
    let typing = shared.palette.is_open() || canvases.editing();
    let page = !shared.modal() && canvases.can_paste(&Clip::default(), &shared.panes);
    tools.flash().say(paste::unpasted(clip, typing, page).into(), Instant::now());
}

/// The canvas pane of `panes` laid in `area` under `at`, a point in the
/// window, and its canvas's frame (above its footer). Over any other pane,
/// or a footer, a gesture goes on to what it shows (an agent UI scrolls itself).
pub fn pane_at(panes: &Panes, area: Rect, at: Point) -> Option<(PaneId, Rect)> {
    let mut placed = panes.place(area).into_iter().map(|(id, p, f)| (id, p, status::content(f)));
    let found = placed.find(|(_, _, frame)| frame.contains(at));
    found.filter(|(_, pane, _)| pane.kind == panes::Kind::Canvas).map(|(id, _, frame)| (id, frame))
}

/// Whether a press of `pen` on the panes of `panes` laid in `area` grabs
/// the selection with the tool of `tools` ([`Canvases::grabs`]): it drags
/// the selection, and focuses its pane, even within reach of a divider.
/// The pen's eraser end never grabs it.
pub fn grabs(canvases: &Canvases, panes: &Panes, tools: &Tools, area: Rect, pen: Pen) -> bool {
    !pen.eraser && canvases.grabs(panes, pane_at(panes, area, pen.at), pen, tools.tool())
}

/// The pointer, up, moved to `at` over the panes of `shared` laid in
/// `area`: on a divider a press would grab (no pop-up open, not grabbing
/// the selection: [`grabs`]), it shows the divider's cursor; on a handle
/// of the selection with no pop-up open, the handle's
/// ([`Canvases::hover_handle`]). Returns whether that changed.
pub fn divider(
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &Tools,
    area: Rect,
    at: Point,
) -> bool {
    let still = Pen { at, when: 0.0, touch: false, eraser: false, shift: false, tablet: false };
    let open = tools.open_kind().is_some();
    let under = pane_at(&shared.panes, area, at).filter(|_| !open);
    let handle = canvases.hover_handle(&shared.panes, under, at, tools.tool());
    let free = !open && !grabs(canvases, &shared.panes, tools, area, still);
    shared.dividers.hover(&shared.panes, area, free.then_some(at)) | handle
}

/// A press of `pen` focuses the pane of `shared` under it, the panes laid
/// beside `sidebar` ([`Shared::press_focus`]), even on a divider when it
/// [`grabs`] the selection; and a done agent's notice goes, as the user is
/// back. Returns whether focus moved or the notice changed.
pub fn press(
    shared: &mut Shared,
    sidebar: &mut Sidebar,
    canvases: &Canvases,
    tools: &Tools,
    pen: Pen,
) -> bool {
    let area = sidebar.panes(shared.size.rect());
    let grabs = grabs(canvases, &shared.panes, tools, area, pen);
    sidebar.pressed() | shared.press_focus(area, pen.at, pen.touch, grabs)
}

/// The frame of the focused pane of `panes` laid in `area`.
pub fn focused(panes: &Panes, area: Rect) -> Rect {
    focus(panes, area).1
}

/// The focused pane of `panes` laid in `area`, and its frame above its footer.
pub fn focus(panes: &Panes, area: Rect) -> (PaneId, Rect) {
    let placed = panes.place(area);
    let found = placed.into_iter().find(|(id, _, _)| *id == panes.focus());
    (panes.focus(), status::content(found.map_or(area, |(_, _, frame)| frame)))
}

/// The window: the sidebar, the panes and their footers (their buttons kept
/// in `shared` for the pen, the one under it lit, with its tip when it is
/// a glyph), any problem with the
/// daemon, a canvas or the config (each said once; the launch line in place
/// of the daemon's, [`scene::daemon`]), the notice, the
/// sling's buttons and what became of it, the pop-up at the pen and the
/// flash of a new tool, a page command, a sling or a saved note, the key
/// overlay, the palette or the page menu on top, the tool's cursor (a
/// selection handle's where the pointer is on one), and the focused pane's canvas and its file in the title bar.
pub fn view(
    settings: &Settings,
    shared: &mut Shared,
    tools: &Tools,
    sidebar: &Sidebar,
    canvases: &mut Canvases,
) -> Box<Scene> {
    let theme = &settings.config.theme;
    let area = sidebar.panes(shared.size.rect());
    canvases.agents(sidebar.agents());
    canvases.follow_paper(settings.config.canvas);
    canvases.set_prompt(&settings.config.prompt);
    canvases.follow_keys(&settings.config.keys);
    let drawn = canvases.show(&shared.panes, area, theme);
    let asking = canvases.asking(shared.trouble.as_ref());
    let own = canvases.own_problems().chain(settings.problem.as_ref());
    let said = scene::daemon::said(sidebar.daemon(), asking, own, theme);
    let (size, uis, notes) = (shared.size, &shared.uis, &shared.notes);
    let mut scene = scene::build(size, &shared.panes, theme, &said, sidebar, drawn, uis, notes);
    canvases.show_live(&mut scene, notes, theme);
    canvases.show_sling(&mut scene, theme, &settings.config.keys, sidebar);
    scene.keyboard = keyboard(shared, canvases);
    scene.labels.splice(0..0, canvases.labels(&shared.panes, area, theme));
    let sling = canvases.sling_marks(&shared.panes, area, theme);
    scene.labels.extend(sling.labels);
    scene.pills.extend(sling.pill);
    let pen = (tools.tool(), tools.colour());
    canvases.set_pen(tools);
    let keys = &settings.config.keys;
    let collapsed = !sidebar.is_open();
    let mut footers = canvases.footers(&shared.panes, area, theme, pen, keys, collapsed);
    footers.extend(sidebar.foot(size.rect()).map(|foot| status::strip(foot, theme)));
    let rows = sidebar.targets(theme, size.rect()).into_iter();
    let mut targets = sidebar.notice_targets(theme, area);
    targets.extend(sling.targets);
    targets.extend(footers.iter().flat_map(|f| f.targets.clone()));
    targets.extend(rows);
    shared.taps.set(targets);
    let lit = shared.taps.lit(theme);
    if let Some((at, _)) = lit {
        status::tip(&mut footers, at, theme, keys, size.rect());
    }
    if let Some(lit) = lit.filter(|&lit| !status::light(&mut footers, lit, theme)) {
        let (frame, fill) = lit;
        let pills = scene.pills.iter_mut().map(|p| &mut p.label);
        let mut buttons = scene.labels.iter_mut().chain(pills);
        if let Some(button) = buttons.find(|l| l.frame == frame && l.fill.is_some()) {
            // A button over the panes: its own fill lit.
            button.fill = Some(fill);
        } else {
            // A workspace's lines: lit under their text, over the shown one's fill.
            let label = Label { text: String::new(), colour: fill, fill: Some(fill), frame };
            let fills = scene.sidebar.iter().take_while(|r| r.label.text.is_empty()).count();
            scene.sidebar.insert(fills, Row { label, font: scene::sidebar::Font::PLAIN });
        }
    }
    scene.footers = footers;
    scene.labels.extend(tools.labels(theme));
    let rows = shared.menu.pane().and_then(|(pane, list)| match list {
        List::Pages => canvases.page_names(&shared.panes, pane).map(|(n, at)| (n, Some(at))),
        List::Profiles => canvases.profile_names(&shared.panes, pane),
    });
    shared.menu.refresh(rows);
    let overlay = shared.overlay.shown(keys, theme, size);
    let overlay = overlay.or_else(|| shared.palette.shown(keys, theme, size));
    let overlay = overlay.or_else(|| shared.menu.shown(theme, size));
    scene.overlay = overlay.or(sling.panel).map(Box::new);
    let free = !shared.modal() && tools.open_kind().is_none();
    for pane in &mut scene.canvases {
        let handle = shared.hand.cursor().or(canvases.handle_cursor(pane.pane).filter(|_| free));
        let tool = || cursor::over_canvas(tools, theme, pane.scene.viewport.zoom());
        pane.cursor = handle.unwrap_or_else(tool);
    }
    scene.divider = shared.dividers.cursor().filter(|_| free);
    let now = Instant::now();
    tools.follow(keys, now);
    let said = canvases.said();
    if let Some(text) = canvases.take_flash().or(said) {
        tools.flash().say(text, now);
    }
    scene.flash = tools.flash().pill(focused(&shared.panes, area), theme, now).map(Box::new);
    let file = canvases.file(&shared.panes, shared.panes.focus());
    scene.title = title::window(file, std::env::home_dir().as_deref());
    let facts = facts(shared, canvases, tools, sidebar, area);
    scene.menus = commands::menu::bar(keys, status::command(tools.chosen()), &facts);
    Box::new(scene)
}

/// Pans or zooms the canvas of the pane of `panes` laid in `area` under a
/// scroll or a pinch; false when no pane is under it, or for another event.
pub fn gesture(canvases: &mut Canvases, panes: &Panes, area: Rect, event: &Event) -> bool {
    let (Event::Scroll { at, .. } | Event::Zoom { at, .. }) = *event else { return false };
    let Some((pane, frame)) = pane_at(panes, area, at) else { return false };
    match *event {
        Event::Scroll { dx, dy, .. } => canvases.pan(pane, dx, dy),
        Event::Zoom { factor, .. } => {
            canvases.zoom(pane, Point { x: at.x - frame.x, y: at.y - frame.y }, factor);
        }
        _ => return false,
    }
    true
}
