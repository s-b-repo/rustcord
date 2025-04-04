use gtk4 as gtk;
use gtk::{Box as GtkBox, HeaderBar, Orientation, MenuButton};
use gtk::prelude::*;
use libadwaita::{Application, Window as AdwWindow};
use crate::gui::{controls::build_controls, preview::build_preview, settings_dialog::SettingsDialog};

pub fn build_ui(app: &Application) {
    let window = AdwWindow::new(app);
    window.set_title(Some("Waycord Recorder – Ultimate Edition"));
    window.set_default_size(1280, 720);

    let header = HeaderBar::new();
    header.set_title(Some("Waycord Screen Recorder"));
    header.set_show_title_buttons(true);
    window.set_titlebar(Some(&header));

    let content = GtkBox::new(Orientation::Vertical, 0);

    // 1) Live preview of the screen
    let preview = build_preview();

    // 2) Recording controls (start/stop/pause, audio sources, resolution, etc.)
    let controls = build_controls();

    // "Settings" button → opens SettingsDialog
    let settings_button = gtk::Button::with_label("Settings");
    settings_button.connect_clicked(glib::clone!(@weak window => move |_| {
        let dialog = SettingsDialog::new(&window);
        dialog.present();
    }));
    header.pack_end(&settings_button);

    // "Camera Position" dropdown
    let camera_button = MenuButton::new();
    camera_button.set_label("Camera Position");
    header.pack_end(&camera_button);

    let menu_model = gio::Menu::new();
    menu_model.append("Top-Left", Some("app.cam_position_top_left"));
    menu_model.append("Bottom-Right", Some("app.cam_position_bottom_right"));
    camera_button.set_menu_model(Some(&menu_model));

    let action_top_left = gio::SimpleAction::new("cam_position_top_left", None);
    action_top_left.connect_activate(move |_, _| {
        println!("Camera: set to top-left. (Adjust overlay or scene geometry in your manager.)");
    });

    let action_bottom_right = gio::SimpleAction::new("cam_position_bottom_right", None);
    action_bottom_right.connect_activate(move |_, _| {
        println!("Camera: set to bottom-right. (Adjust overlay or scene geometry in your manager.)");
    });

    app.add_action(&action_top_left);
    app.add_action(&action_bottom_right);

    // Put everything in content
    content.append(&preview);
    content.append(&controls);

    window.set_content(Some(&content));
    window.present();
}
