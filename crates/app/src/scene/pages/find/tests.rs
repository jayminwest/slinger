use super::*;

fn pages() -> Vec<Page> {
    let page = |n: u32, name: &str| Page { id: format!("p{n}"), name: name.into() };
    vec![page(1, "ideas"), page(2, "spec"), page(3, "notes")]
}

fn hit(page: &str, item: u64) -> Hit {
    Hit {
        page: page.into(),
        item: ItemId(item),
        frame: Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        folded: None,
    }
}

#[test]
fn a_find_starts_on_the_shown_page_or_after_it_and_goes_round() {
    let hits = [hit("p1", 4), hit("p3", 2), hit("p3", 9)];
    let pages = pages();
    assert_eq!(next(&hits, &pages, "p1", None), Some(0));
    assert_eq!(next(&hits, &pages, "p2", None), Some(1), "none on p2: the next page's");
    assert_eq!(next(&[hit("p1", 4)], &pages, "p3", None), Some(0), "after the last, the first");
    assert_eq!(next(&hits, &pages, "p7", None), Some(0), "a page not known counts as the first");
    assert_eq!(next(&[], &pages, "p1", None), None);
}

#[test]
fn find_next_goes_to_the_text_after_the_last_and_round() {
    let hits = [hit("p1", 4), hit("p3", 2), hit("p3", 9)];
    let pages = pages();
    assert_eq!(next(&hits, &pages, "p1", Some(ItemId(4))), Some(1));
    assert_eq!(next(&hits, &pages, "p3", Some(ItemId(2))), Some(2));
    assert_eq!(next(&hits, &pages, "p3", Some(ItemId(9))), Some(0), "after the last, the first");
    assert_eq!(next(&hits, &pages, "p2", Some(ItemId(5))), Some(1), "one gone: from the page");
}

#[test]
fn the_flash_names_the_words_the_place_among_the_hits_and_the_page() {
    let hits = [hit("p1", 4), hit("p3", 2)];
    let pages = pages();
    assert_eq!(said("plan", &hits, Some(1), &pages), "“plan” 2/2 · notes");
    assert_eq!(said("plan", &[hit("p8", 1)], Some(0), &pages), "“plan” 1/1 · p8");
    assert_eq!(said("plan", &[], None, &pages), "No text holds “plan”");
    let hidden = Hit { folded: Some(ItemId(7)), ..hit("p1", 4) };
    assert_eq!(said("plan", &[hidden], Some(0), &pages), "“plan” 1/1 · ideas · in a folded box");
}
