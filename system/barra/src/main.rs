//! La barra de intención: la primera superficie del escritorio de syso.
//!
//! No es un lanzador de aplicaciones. Es el mismo recorrido de siempre —plan,
//! diff, nivel de permiso, aprobación— con otra interfaz, hablando por el
//! socket con el mismo demonio que atiende al terminal.
//!
//! ## Por qué layer-shell y no una ventana normal
//!
//! `wlr-layer-shell` permite dibujar superficies que forman parte del shell:
//! flotan sobre todo, se anclan a un borde y pueden pedir el teclado. Es lo
//! que separa «una ventana de una aplicación» de «una pieza del escritorio».
//! Y no hace falta escribir un compositor para tenerlo.
//!
//! ## La decisión de diseño que más importa
//!
//! En el terminal escribes «s»; aquí es un clic, y los clics son baratos. El
//! documento de arquitectura ya avisaba de que la confirmación se degrada con
//! el uso, y una interfaz gráfica empeora ese riesgo.
//!
//! Por eso el diff NO es un diálogo que se cierra: es el contenido de la
//! superficie. El botón de aprobar vive DEBAJO del diff, así que para llegar
//! a él hay que haberlo tenido delante. Y el nivel de permiso no es una
//! etiqueta más — tiñe el borde de la hoja entera dentro de la que lees.

use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as Caja, Button, CssProvider, Entry, Label,
    Orientation, PolicyType, ScrolledWindow,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use syso_protocolo::{Evento, Line, Peticion, Propuesta, Tier};

const ANCHO: i32 = 720;

fn ruta_socket() -> PathBuf {
    if let Some(v) = std::env::var_os("SYSO_SOCKET") {
        return PathBuf::from(v);
    }
    let estado = std::env::var_os("SYSO_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".syso"));
    estado.join("syso.sock")
}

fn main() {
    let app = Application::builder()
        .application_id("dev.syso.barra")
        .build();

    app.connect_startup(|_| {
        let proveedor = CssProvider::new();
        proveedor.load_from_string(include_str!("estilo.css"));
        if let Some(pantalla) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &pantalla,
                &proveedor,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    app.connect_activate(construir);

    // Sin argumentos: los que reciba la barra son suyos, no de GTK.
    app.run_with_args::<&str>(&[]);
}

fn construir(app: &Application) {
    let ventana = ApplicationWindow::builder()
        .application(app)
        .default_width(ANCHO)
        .build();
    ventana.add_css_class("fondo");

    // Aquí es donde deja de ser una ventana y pasa a ser parte del escritorio.
    ventana.init_layer_shell();
    ventana.set_layer(Layer::Overlay);
    // Exclusivo porque hay que poder escribir: una barra de intención que no
    // recibe teclado no sirve para nada.
    ventana.set_keyboard_mode(KeyboardMode::Exclusive);
    ventana.set_anchor(Edge::Top, true);
    ventana.set_margin(Edge::Top, 120);

    let marco = Caja::new(Orientation::Vertical, 12);
    marco.add_css_class("marco");
    ventana.set_child(Some(&marco));

    let entrada = Entry::builder()
        .placeholder_text("¿qué quieres que haga?")
        .build();
    entrada.add_css_class("intencion");
    marco.append(&entrada);

    // Todo lo que el demonio conteste se dibuja aquí debajo.
    let contenido = Caja::new(Orientation::Vertical, 10);
    marco.append(&contenido);

    let escritura: Rc<RefCell<Option<UnixStream>>> = Rc::new(RefCell::new(None));

    {
        let contenido = contenido.clone();
        let escritura = escritura.clone();
        let entrada_ref = entrada.clone();
        entrada.connect_activate(move |campo| {
            let texto = campo.text().to_string();
            if texto.trim().is_empty() {
                return;
            }
            entrada_ref.set_sensitive(false);
            vaciar(&contenido);

            match arrancar_sesion(&texto) {
                Ok((flujo, eventos)) => {
                    *escritura.borrow_mut() = Some(flujo);
                    dibujar_espera(&contenido);
                    escuchar(eventos, contenido.clone(), escritura.clone(), entrada_ref.clone());
                }
                Err(e) => mostrar_error(&contenido, &format!("{e}")),
            }
        });
    }

    // Escape cierra sin contestar. El demonio lee el fin de la conexión y lo
    // trata como un no: el silencio nunca es un sí.
    let controlador = gtk4::EventControllerKey::new();
    let ventana_ref = ventana.clone();
    controlador.connect_key_pressed(move |_, tecla, _, _| {
        if tecla == gtk4::gdk::Key::Escape {
            ventana_ref.close();
            return gtk4::glib::Propagation::Stop;
        }
        gtk4::glib::Propagation::Proceed
    });
    ventana.add_controller(controlador);

    ventana.present();
}

/// Abre la conexión y deja un hilo leyendo eventos.
fn arrancar_sesion(texto: &str) -> Result<(UnixStream, Receiver<Evento>), String> {
    let ruta = ruta_socket();
    let flujo = UnixStream::connect(&ruta)
        .map_err(|e| format!("no hay demonio en {}: {e}\nArráncalo con: syso demonio", ruta.display()))?;

    let mut escritura = flujo.try_clone().map_err(|e| e.to_string())?;
    let peticion = Peticion::Intencion {
        texto: texto.to_string(),
        planificador: None,
        seco: false,
    };
    writeln!(escritura, "{}", serde_json::to_string(&peticion).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    escritura.flush().map_err(|e| e.to_string())?;

    // El socket se lee en un hilo aparte: bloquear el bucle de GTK dejaría la
    // superficie congelada mientras el modelo piensa.
    let (emisor, receptor) = channel();
    let lectura = flujo.try_clone().map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let mut buffer = BufReader::new(lectura);
        loop {
            let mut linea = String::new();
            match buffer.read_line(&mut linea) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(evento) = serde_json::from_str::<Evento>(linea.trim()) {
                        if emisor.send(evento).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok((escritura, receptor))
}

fn escuchar(
    eventos: Receiver<Evento>,
    contenido: Caja,
    escritura: Rc<RefCell<Option<UnixStream>>>,
    entrada: Entry,
) {
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
        while let Ok(evento) = eventos.try_recv() {
            match evento {
                Evento::Inicio { .. } => {}
                Evento::Nota(texto) => {
                    vaciar(&contenido);
                    contenido.append(&etiqueta(&format!("dice: {texto}"), "radio"));
                }
                Evento::Propuesta(propuesta) => {
                    vaciar(&contenido);
                    dibujar_propuesta(&contenido, &propuesta, escritura.clone());
                }
                Evento::Salida(texto) => contenido.append(&etiqueta(&texto, "paso")),
                Evento::Resultado(resultado) => {
                    let clase = if resultado.ok { "ok" } else { "error" };
                    contenido.append(&etiqueta(&resultado.mensaje, clase));
                    entrada.set_sensitive(true);
                    entrada.set_text("");
                }
                Evento::Error(mensaje) => {
                    mostrar_error(&contenido, &mensaje);
                    entrada.set_sensitive(true);
                }
            }
        }
        gtk4::glib::ControlFlow::Continue
    });
}

fn dibujar_propuesta(
    contenido: &Caja,
    propuesta: &Propuesta,
    escritura: Rc<RefCell<Option<UnixStream>>>,
) {
    let hoja = Caja::new(Orientation::Vertical, 10);
    hoja.add_css_class("hoja");
    hoja.add_css_class(clase_nivel(propuesta.nivel));

    // 1 · el plan
    hoja.append(&etiqueta("PLAN", "etiqueta"));
    for (i, paso) in propuesta.plan.steps.iter().enumerate() {
        let args = paso
            .args
            .iter()
            .map(|(k, v)| format!("{k}={}", recorta(v, 36)))
            .collect::<Vec<_>>()
            .join(" ");
        hoja.append(&etiqueta(
            &format!("{}. {}  {args}", i + 1, paso.capability),
            "paso",
        ));
    }

    // 2 · el diff, que es el contenido de esta superficie
    hoja.append(&etiqueta("CAMBIOS", "etiqueta"));
    let lista = Caja::new(Orientation::Vertical, 0);
    for linea in &propuesta.cambios {
        let (texto, clase) = match linea {
            Line::Info(t) => (t.clone(), "info"),
            Line::Add(t) => (format!("+{t}"), "mas"),
            Line::Del(t) => (format!("-{t}"), "menos"),
        };
        let l = etiqueta(&texto, "diff");
        l.add_css_class(clase);
        lista.append(&l);
    }
    let desplazable = ScrolledWindow::builder()
        .child(&lista)
        .min_content_height(120)
        .max_content_height(360)
        .propagate_natural_height(true)
        .hscrollbar_policy(PolicyType::Automatic)
        .build();
    hoja.append(&desplazable);

    // 3 · el radio de impacto y el nivel
    hoja.append(&etiqueta("RADIO DE IMPACTO", "etiqueta"));
    for (nombre, rutas) in [
        ("escribe", &propuesta.radio.escribe),
        ("borra", &propuesta.radio.borra),
        ("lee", &propuesta.radio.lee),
        ("SISTEMA", &propuesta.radio.sistema),
        ("red", &propuesta.radio.red),
    ] {
        if !rutas.is_empty() {
            hoja.append(&etiqueta(
                &format!("{nombre:9} {}", recorta(&rutas.join(", "), 70)),
                "radio",
            ));
        }
    }
    let nivel = etiqueta(
        &format!(
            "nivel     {} — {}",
            propuesta.nivel.label(),
            propuesta.razones.join("; ")
        ),
        "nivel",
    );
    nivel.add_css_class(clase_nivel(propuesta.nivel));
    hoja.append(&nivel);
    hoja.append(&etiqueta(
        &format!(
            "recinto   {} — {}",
            propuesta.recinto.motor, propuesta.recinto.garantiza
        ),
        "radio",
    ));

    contenido.append(&hoja);

    // 4 · y solo entonces, la decisión
    if propuesta.nivel == Tier::Auto || propuesta.seco {
        return;
    }

    let botones = Caja::new(Orientation::Horizontal, 10);
    botones.set_halign(Align::End);

    let descartar = Button::with_label("Descartar");
    descartar.add_css_class("descartar");
    let aprobar = Button::with_label("Aprobar");
    aprobar.add_css_class("aprobar");

    for (boton, decision) in [(&descartar, false), (&aprobar, true)] {
        let escritura = escritura.clone();
        let botones_ref = botones.clone();
        boton.connect_clicked(move |_| {
            if let Some(flujo) = escritura.borrow_mut().as_mut() {
                let respuesta = Peticion::Aprobacion(decision);
                if let Ok(json) = serde_json::to_string(&respuesta) {
                    let _ = writeln!(flujo, "{json}");
                    let _ = flujo.flush();
                }
            }
            // Una decisión se toma una vez.
            botones_ref.set_sensitive(false);
        });
    }

    botones.append(&descartar);
    botones.append(&aprobar);
    contenido.append(&botones);
}

// ------------------------------------------------------------------ ayudas

fn clase_nivel(nivel: Tier) -> &'static str {
    match nivel {
        Tier::Auto => "auto",
        Tier::Confirm => "confirm",
        Tier::Grant => "grant",
    }
}

fn etiqueta(texto: &str, clase: &str) -> Label {
    let l = Label::new(Some(texto));
    l.set_halign(Align::Start);
    l.set_xalign(0.0);
    l.set_selectable(true);
    l.add_css_class(clase);
    l
}

fn dibujar_espera(contenido: &Caja) {
    contenido.append(&etiqueta("pensando…", "radio"));
}

fn mostrar_error(contenido: &Caja, mensaje: &str) {
    vaciar(contenido);
    for linea in mensaje.lines() {
        contenido.append(&etiqueta(linea, "error"));
    }
}

fn vaciar(caja: &Caja) {
    while let Some(hijo) = caja.first_child() {
        caja.remove(&hijo);
    }
}

fn recorta(s: &str, max: usize) -> String {
    let plano = s.replace('\n', "⏎");
    if plano.chars().count() <= max {
        plano
    } else {
        format!("{}…", plano.chars().take(max - 1).collect::<String>())
    }
}
