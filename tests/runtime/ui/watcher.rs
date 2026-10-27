use aidget::runtime::ui::is_relevant;

#[test]
fn ignores_state_and_run_events() {
    use notify::event::{AccessKind, AccessMode, CreateKind, Event, EventKind};
    let state = Event::new(EventKind::Create(CreateKind::File)).add_path(std::path::PathBuf::from(
        "/tmp/aidget/state/screenshots/a.png",
    ));
    assert!(!is_relevant(&Ok(state)));

    let widget = Event::new(EventKind::Create(CreateKind::File))
        .add_path(std::path::PathBuf::from("/tmp/aidget/widgets/a/index.html"));
    assert!(is_relevant(&Ok(widget)));

    let access = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
        .add_path(std::path::PathBuf::from("/tmp/aidget/widgets/a/index.html"));
    assert!(!is_relevant(&Ok(access)));
}
