//! The application menu on macOS, in the language of the app. Everything the system would call by
//! its own name is written here, so the menu agrees with the window.

/// The words of the menu in one language. A few are the start of a name: "Quit" + the app.
pub struct MenuLabels {
    pub about: &'static str,
    pub settings: &'static str,
    pub check_updates: &'static str,
    pub services: &'static str,
    pub hide: &'static str,
    pub hide_others: &'static str,
    pub show_all: &'static str,
    pub quit: &'static str,
    pub file: &'static str,
    pub close_tab: &'static str,
    pub close_window: &'static str,
    pub edit: &'static str,
    pub undo: &'static str,
    pub redo: &'static str,
    pub cut: &'static str,
    pub copy: &'static str,
    pub paste: &'static str,
    pub select_all: &'static str,
    pub view: &'static str,
    pub fullscreen: &'static str,
    pub window: &'static str,
    pub minimize: &'static str,
    pub maximize: &'static str,
}

impl MenuLabels {
    /// Every label, for checking that none is missing.
    #[cfg(test)]
    fn all(&self) -> [&'static str; 23] {
        [
            self.about, self.settings, self.check_updates, self.services, self.hide, self.hide_others, self.show_all, self.quit, self.file, self.close_tab, self.close_window, self.edit, self.undo,
            self.redo, self.cut, self.copy, self.paste, self.select_all, self.view, self.fullscreen, self.window, self.minimize, self.maximize,
        ]
    }
}

const ENGLISH: MenuLabels = MenuLabels {
    about: "About",
    settings: "Settings…",
    check_updates: "Check for Updates…",
    services: "Services",
    hide: "Hide",
    hide_others: "Hide Others",
    show_all: "Show All",
    quit: "Quit",
    file: "File",
    close_tab: "Close Tab",
    close_window: "Close Window",
    edit: "Edit",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    select_all: "Select All",
    view: "View",
    fullscreen: "Enter Full Screen",
    window: "Window",
    minimize: "Minimize",
    maximize: "Zoom",
};

const PORTUGUESE: MenuLabels = MenuLabels {
    about: "Sobre o",
    settings: "Configurações…",
    check_updates: "Verificar atualizações…",
    services: "Serviços",
    hide: "Ocultar o",
    hide_others: "Ocultar outros",
    show_all: "Mostrar tudo",
    quit: "Encerrar o",
    file: "Arquivo",
    close_tab: "Fechar aba",
    close_window: "Fechar janela",
    edit: "Editar",
    undo: "Desfazer",
    redo: "Refazer",
    cut: "Recortar",
    copy: "Copiar",
    paste: "Colar",
    select_all: "Selecionar tudo",
    view: "Visualizar",
    fullscreen: "Entrar em tela cheia",
    window: "Janela",
    minimize: "Minimizar",
    maximize: "Zoom",
};

/// The labels of a language (`en`, `pt-BR`); any Portuguese reads the Brazilian ones, anything unknown is English.
pub fn labels(lang: &str) -> &'static MenuLabels {
    if lang.to_ascii_lowercase().starts_with("pt") {
        &PORTUGUESE
    } else {
        &ENGLISH
    }
}

/// Menu item ids and the event the frontend listens to for Cmd+W.
#[cfg(target_os = "macos")]
pub const CLOSE_TAB_ID: &str = "close-tab";
#[cfg(target_os = "macos")]
pub const CLOSE_WINDOW_ID: &str = "close-window";
#[cfg(target_os = "macos")]
pub const CLOSE_TAB_EVENT: &str = "menu:close-tab";
#[cfg(target_os = "macos")]
pub const SETTINGS_ID: &str = "settings";
#[cfg(target_os = "macos")]
pub const CHECK_UPDATES_ID: &str = "check-updates";

/// Tauri's default macOS menu, except Cmd+W closes the active connection tab
/// (handled by the frontend) and Close Window moves to Cmd+Shift+W. The app menu also
/// opens Settings (Cmd+,) and checks for updates; the frontend gets `menu:<id>`.
#[cfg(target_os = "macos")]
pub fn build<R: tauri::Runtime>(app: &tauri::AppHandle<R>, lang: &str) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

    let words = labels(lang);
    let pkg_info = app.package_info();
    let name = pkg_info.name.clone();
    let about = AboutMetadata {
        name: Some(name.clone()),
        version: Some(pkg_info.version.to_string()),
        ..Default::default()
    };

    let settings = MenuItemBuilder::with_id(SETTINGS_ID, words.settings).accelerator("CmdOrCtrl+,").build(app)?;
    let check_updates = MenuItemBuilder::with_id(CHECK_UPDATES_ID, words.check_updates).build(app)?;
    let app_menu = SubmenuBuilder::new(app, &name)
        .item(&PredefinedMenuItem::about(app, Some(&format!("{} {name}", words.about)), Some(about))?)
        .item(&check_updates)
        .separator()
        .item(&settings)
        .separator()
        .item(&PredefinedMenuItem::services(app, Some(words.services))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some(&format!("{} {name}", words.hide)))?)
        .item(&PredefinedMenuItem::hide_others(app, Some(words.hide_others))?)
        .item(&PredefinedMenuItem::show_all(app, Some(words.show_all))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some(&format!("{} {name}", words.quit)))?)
        .build()?;

    let close_tab = MenuItemBuilder::with_id(CLOSE_TAB_ID, words.close_tab).accelerator("CmdOrCtrl+W").build(app)?;
    let close_window = MenuItemBuilder::with_id(CLOSE_WINDOW_ID, words.close_window).accelerator("CmdOrCtrl+Shift+W").build(app)?;
    let file_menu = SubmenuBuilder::new(app, words.file).item(&close_tab).item(&close_window).build()?;

    let edit_menu = SubmenuBuilder::new(app, words.edit)
        .item(&PredefinedMenuItem::undo(app, Some(words.undo))?)
        .item(&PredefinedMenuItem::redo(app, Some(words.redo))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, Some(words.cut))?)
        .item(&PredefinedMenuItem::copy(app, Some(words.copy))?)
        .item(&PredefinedMenuItem::paste(app, Some(words.paste))?)
        .item(&PredefinedMenuItem::select_all(app, Some(words.select_all))?)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, words.view).item(&PredefinedMenuItem::fullscreen(app, Some(words.fullscreen))?).build()?;

    let window_menu = SubmenuBuilder::new(app, words.window)
        .item(&PredefinedMenuItem::minimize(app, Some(words.minimize))?)
        .item(&PredefinedMenuItem::maximize(app, Some(words.maximize))?)
        .build()?;

    MenuBuilder::new(app).items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu]).build()
}

/// Shows the menu in another language; the interface calls it when the language of the app changes.
#[tauri::command]
pub fn set_menu_language(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let menu = build(&app, &lang).map_err(|e| e.to_string())?;
        app.set_menu(menu).map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&app, &lang);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_label_for_every_item() {
        for lang in ["en", "pt-BR"] {
            for (index, label) in labels(lang).all().iter().enumerate() {
                assert!(!label.trim().is_empty(), "{lang} has no label at {index}");
            }
        }
    }

    #[test]
    fn an_unknown_language_uses_english() {
        assert_eq!(labels("fr").settings, labels("en").settings);
        assert_eq!(labels("").settings, "Settings…");
    }

    #[test]
    fn any_portuguese_uses_the_brazilian_labels() {
        assert_eq!(labels("pt").settings, labels("pt-BR").settings);
        assert_eq!(labels("pt-PT").file, labels("pt-BR").file);
    }

    #[test]
    fn portuguese_labels_differ_from_english_for_the_apps_own_items() {
        let (en, pt) = (labels("en"), labels("pt-BR"));
        for (english, portuguese) in [(en.settings, pt.settings), (en.check_updates, pt.check_updates), (en.close_tab, pt.close_tab), (en.close_window, pt.close_window), (en.file, pt.file), (en.edit, pt.edit), (en.view, pt.view), (en.window, pt.window)] {
            assert_ne!(english, portuguese);
        }
        assert_eq!(pt.settings, "Configurações…");
    }
}
