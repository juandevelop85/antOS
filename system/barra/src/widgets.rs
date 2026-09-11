//! Helpers de renderizado compartidos por el resto de módulos de la barra:
//! etiquetas, contenedores vacíos, y el mapeo de `Tier` a clase CSS.

use antos_protocol::Tier;
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Label};

pub(crate) fn level_css_class(tier: Tier) -> &'static str {
    match tier {
        Tier::Auto => "auto",
        Tier::Confirm => "confirm",
        Tier::Grant => "grant",
    }
}

pub(crate) fn make_label(text: &str, class_name: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_halign(Align::Start);
    l.set_xalign(0.0);
    l.set_selectable(true);
    l.add_css_class(class_name);
    l
}

pub(crate) fn render_waiting(content: &GtkBox) {
    content.append(&make_label("antOS pensando...", "radio"));
}

pub(crate) fn render_error(content: &GtkBox, message: &str) {
    empty_box(content);
    for line in message.lines() {
        content.append(&make_label(line, "error"));
    }
}

pub(crate) fn empty_box(gtk_box: &GtkBox) {
    while let Some(child) = gtk_box.first_child() {
        gtk_box.remove(&child);
    }
}

pub(crate) fn truncate_str(s: &str, max_len: usize) -> String {
    let flat = s.replace('\n', "⏎");
    if flat.chars().count() <= max_len {
        flat
    } else {
        format!("{}…", flat.chars().take(max_len - 1).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    //! Pruebas de la lógica pura de `antos-barra` que no depende de GTK4
    //! (T31.13). No se pudieron compilar ni ejecutar en esta máquina de
    //! desarrollo (macOS, sin `gtk4-layer-shell` — ver T31.10): quedan a la
    //! espera de verificación en el runner de CI de Linux.
    use super::*;

    #[test]
    fn test_truncate_str_keeps_short_strings_unchanged() {
        assert_eq!(truncate_str("antOS", 10), "antOS");
    }

    #[test]
    fn test_truncate_str_cuts_long_strings_and_appends_an_ellipsis() {
        let truncated = truncate_str("abcdefghij", 5);
        assert_eq!(truncated, "abcd…");
        assert_eq!(truncated.chars().count(), 5);
    }

    #[test]
    fn test_truncate_str_replaces_newlines_before_measuring_length() {
        // Un salto de línea se sustituye por «⏎» antes de contar caracteres,
        // así una entrada multilínea no revienta el layout de una sola fila.
        assert_eq!(truncate_str("a\nb", 10), "a⏎b");
    }

    #[test]
    fn test_level_css_class_maps_every_tier_to_its_css_class() {
        assert_eq!(level_css_class(Tier::Auto), "auto");
        assert_eq!(level_css_class(Tier::Confirm), "confirm");
        assert_eq!(level_css_class(Tier::Grant), "grant");
    }
}
