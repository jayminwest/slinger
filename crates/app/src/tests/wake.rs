//! A backend thread's wake takes the same path as the timer's tick: the
//! window looks at the news (slinger-1e161b).

use super::*;

#[test]
fn a_wake_and_a_tick_look_at_the_news() {
    let settings = Settings::load(ConfigFiles::at("/nonexistent/config.toml".into()));
    let mut tools = Tools::new(settings.config.palette.clone());
    let (mut shared, mut sidebar) = (Shared::default(), Sidebar::new(false));
    let mut canvases = Canvases::new(control::Client::new("/nonexistent/x.sock"));
    let socket = Path::new("/nonexistent/x.sock");
    for event in [Event::Wake, Event::Tick] {
        let routed =
            route(event, &mut shared, &mut canvases, &mut tools, &settings, socket, &mut sidebar);
        assert!(matches!(routed, Routed::News(false)));
    }
}
