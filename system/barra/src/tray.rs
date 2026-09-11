//! Icono de bandeja de sistema (`StatusNotifierItem`) para antos-barra: un
//! click alterna expandir/colapsar la ventana — el mismo comportamiento que
//! el icono de Claude en la barra de menú de macOS (T30.8, seguimiento).
//!
//! Aparece dentro de cualquier bandeja compatible con SNI, por ejemplo el
//! módulo `tray` que el panel de waybar ya trae declarado (T30.6,
//! `system/nixos/desktop.nix`) — no hace falta tocar esa configuración.

use ksni::blocking::TrayMethods;
use ksni::{Icon, Tray};
use std::sync::mpsc::Sender;

const ICON_SIZE: i32 = 22;

struct AntosTray {
    activate_tx: Sender<()>,
}

impl Tray for AntosTray {
    fn id(&self) -> String {
        "dev.antos.barra".into()
    }

    fn title(&self) -> String {
        "antOS".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![antos_icon()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.activate_tx.send(());
    }
}

/// Cuadrado sólido de 22×22 con el verde de acento de antOS (`.ok` /
/// `.git-badge.clean` en `estilo.css`, `#56BCA0`) — ARGB32 en orden de red,
/// generado a mano para no depender de un tema de iconos del sistema ni de
/// un crate de imágenes.
fn antos_icon() -> Icon {
    let pixel = [0xFFu8, 0x56, 0xBC, 0xA0]; // A R G B
    let mut data = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for _ in 0..(ICON_SIZE * ICON_SIZE) {
        data.extend_from_slice(&pixel);
    }
    Icon {
        width: ICON_SIZE,
        height: ICON_SIZE,
        data,
    }
}

/// Publica el icono en la bandeja SNI del sistema, en un hilo aparte:
/// `TrayMethods::spawn` bloquea el hilo llamante hasta completar el
/// registro D-Bus inicial, y no debe hacerlo sobre el hilo principal de
/// GTK. `assume_sni_available(true)` trata la ausencia inicial del
/// `StatusNotifierWatcher` como un error blando que se reintenta solo
/// (comportamiento por defecto de `Tray::watcher_offline`) en vez de
/// rendirse — la misma clase de carrera de arranque que ya se vio con
/// `zwlr_layer_shell_v1` (T30.8).
///
/// Si el registro SNI falla del todo, la barra sigue funcionando igual —
/// D-Bus/SNI es un atajo para abrirla, no un requisito para que exista.
pub(crate) fn spawn(activate_tx: Sender<()>) {
    std::thread::spawn(move || {
        let tray = AntosTray { activate_tx };
        match tray.assume_sni_available(true).spawn() {
            Ok(handle) => {
                // El servicio ya corre en su propio hilo (ver
                // `TrayMethods::spawn`); nada en este proceso vuelve a
                // tocar el `Handle`, así que se filtra a propósito en vez
                // de dejarlo caer sin saber si `Drop` apaga el servicio.
                std::mem::forget(handle);
            }
            Err(err) => {
                eprintln!("antOS · aviso: no se pudo publicar el icono de bandeja SNI: {err}");
            }
        }
    });
}
