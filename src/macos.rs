use gpui::*;

actions!(herdr, [Quit, Hide, HideOthers, ShowAll, Settings]);

pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Settings, _| crate::mods::request(crate::mods::Request::Run { name: "app.settings".into(), args: serde_json::Value::Null }));
    cx.set_menus([Menu::new("Herdr").items([
        MenuItem::action("Settings", Settings),
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Hide Herdr", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action("Quit Herdr", Quit),
    ]), Menu::new("Edit").items([
        // Native responder actions reach WKWebView's text fields; GPUI's
        // input actions remain the fallback for the address field.
        MenuItem::os_action("Undo", gpui_base::input::Undo, OsAction::Undo),
        MenuItem::os_action("Redo", gpui_base::input::Redo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Cut", gpui_base::input::Cut, OsAction::Cut),
        MenuItem::os_action("Copy", gpui_base::input::Copy, OsAction::Copy),
        MenuItem::os_action("Paste", gpui_base::input::Paste, OsAction::Paste),
        MenuItem::os_action("Select All", gpui_base::input::SelectAll, OsAction::SelectAll),
    ])]);
}

pub fn action(name: &str) -> Option<Box<dyn Action>> {
    match name {
        "app.quit" => Some(Box::new(Quit)),
        "app.hide" => Some(Box::new(Hide)),
        "app.hide_others" => Some(Box::new(HideOthers)),
        "app.show_all" => Some(Box::new(ShowAll)),
        "app.settings" => Some(Box::new(Settings)),
        _ => None,
    }
}
