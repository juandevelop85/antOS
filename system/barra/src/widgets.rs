//! Helpers de renderizado compartidos por el resto de módulos de la barra:
//! etiquetas, contenedores vacíos, y el mapeo de `Tier` a clase CSS.

use antos_protocol::Tier;
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Label};

pub(crate) fn level_css_class(tier: Tier) -> &'static str {
    match tier {
        Tier::Auto => "auto",
        Tier::Confirm => "confirm",
        Tier::Grant => "grant",
    }
}

/// Etiqueta corriente: **no** seleccionable.
///
/// Que no lo sea es el arreglo de dos síntomas que se notaron usando la
/// barra (2026-09-25): una `Label` seleccionable de GTK4 es focusable y
/// muestra el cursor de escritura, así que las filas pulsables construidas
/// con ellas —las tarjetas del lanzador, por ejemplo— salían con cursor de
/// texto y, al pulsarlas, le robaban el foco a la `Entry`; después ya no se
/// podía escribir sin volver a pinchar en ella.
///
/// El texto que el usuario querrá copiar (la respuesta del sistema, los
/// diffs, los errores) usa [`make_selectable_label`], que sí lo es —ahí la
/// selección es la funcionalidad, no un efecto secundario.
pub(crate) fn make_label(text: &str, class_name: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_halign(Align::Start);
    l.set_xalign(0.0);
    l.add_css_class(class_name);
    l
}

/// Etiqueta de salida que el usuario puede seleccionar y copiar. Solo para
/// texto que se lee, nunca dentro de algo pulsable (ver [`make_label`]).
pub(crate) fn make_selectable_label(text: &str, class_name: &str) -> Label {
    let l = make_label(text, class_name);
    l.set_selectable(true);
    l
}

/// Botón de acción de la barra.
///
/// `focus_on_click(false)` es la razón de que esto exista: sin ello, pulsar
/// cualquier botón —una píldora de sugerencia, «Aprobar», «Detener»— le
/// pasaba el foco al botón y la `Entry` dejaba de recibir lo que se
/// escribía (2026-09-25). La barra es una caja de escribir con botones
/// alrededor: el foco es de la `Entry` y ahí se queda.
pub(crate) fn action_button(label: &str, class_name: &str) -> Button {
    let b = Button::with_label(label);
    b.set_focus_on_click(false);
    if !class_name.is_empty() {
        b.add_css_class(class_name);
    }
    b
}

pub(crate) fn render_waiting(content: &GtkBox) {
    content.append(&make_label("antOS pensando...", "radio"));
}

pub(crate) fn render_error(content: &GtkBox, message: &str) {
    empty_box(content);
    for line in message.lines() {
        // Un error es justo lo que uno quiere copiar para buscarlo o
        // pegarlo en un ticket.
        content.append(&make_selectable_label(line, "error"));
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
