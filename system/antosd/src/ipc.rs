//! El demonio y su cliente.
//!
//! `antosd` es el demonio central del sistema operativo antOS.
//!
//! ## Por qué ahora
//!
//! Un escritorio necesita el mismo recorrido movido por otra interfaz. Sin
//! esta separación habría que duplicarlo, y duplicar un recorrido es duplicar
//! sus garantías — la puerta de confirmación, el radio de impacto, la
//! instantánea. Dos copias de una garantía acaban siendo una.
//!
//! ## Qué NO cambia
//!
//! El demonio decide el nivel de permiso y comprueba las concesiones. Un
//! cliente solo puede contestar sí o no a una propuesta: no puede fabricar un
//! nivel más bajo ni saltarse la puerta, porque no es él quien la pone.
//!
//! ## Quién puede conectarse
//!
//! El socket vive en el directorio de estado con permisos 0600. Esa es hoy
//! toda la autorización: quien pueda abrir el fichero puede pedir cosas. Es
//! suficiente para un solo usuario en su máquina y claramente insuficiente
//! para cualquier otra cosa.

use crate::capability::Catalog;
use crate::ctx::Ctx;
use crate::protocolo::{Interlocutor, Propuesta, Resultado};
use crate::{sesion, terminal};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

pub fn ruta_socket(ctx: &Ctx) -> PathBuf {
    ctx.state.join("antos.sock")
}

// ------------------------------------------------------------- el protocolo

pub use antos_protocolo::{Evento, Peticion};

/// Una línea de JSON por mensaje. Sin marco binario ni longitudes: se puede
/// leer con `nc` y depurar mirándolo, que a esta escala vale más que los
/// bytes que ahorraría.
fn enviar<T: Serialize>(destino: &mut impl Write, mensaje: &T) -> Result<()> {
    writeln!(destino, "{}", serde_json::to_string(mensaje)?)?;
    destino.flush()?;
    Ok(())
}

fn recibir<T: for<'a> Deserialize<'a>>(origen: &mut impl BufRead) -> Result<Option<T>> {
    let mut linea = String::new();
    if origen.read_line(&mut linea)? == 0 {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(linea.trim())?))
}

// ------------------------------------------------------------- lado servidor

/// El interlocutor del demonio: en vez de pintar, escribe por el socket; y
/// para preguntar, espera una respuesta por él.
struct PorSocket<'a> {
    escritura: &'a mut UnixStream,
    lectura: &'a mut BufReader<UnixStream>,
}

impl Interlocutor for PorSocket<'_> {
    fn inicio(&mut self, intencion: &str, planificador: &str) -> Result<()> {
        enviar(
            self.escritura,
            &Evento::Inicio {
                intencion: intencion.to_string(),
                planificador: planificador.to_string(),
            },
        )
    }

    fn nota(&mut self, texto: &str) -> Result<()> {
        enviar(self.escritura, &Evento::Nota(texto.to_string()))
    }

    fn propone(&mut self, propuesta: &Propuesta) -> Result<bool> {
        // No se puede serializar una referencia prestada dentro del enum sin
        // clonar, así que se reconstruye. Es una vez por plan.
        let copia: Propuesta = serde_json::from_str(&serde_json::to_string(propuesta)?)?;
        enviar(self.escritura, &Evento::Propuesta(Box::new(copia)))?;

        match recibir::<Peticion>(self.lectura)? {
            Some(Peticion::Aprobacion(decision)) => Ok(decision),
            // Un cliente que se va sin contestar no aprueba nada. El silencio
            // nunca es un sí.
            _ => Ok(false),
        }
    }

    fn salida(&mut self, texto: &str) -> Result<()> {
        enviar(self.escritura, &Evento::Salida(texto.to_string()))
    }

    fn resultado(&mut self, resultado: &Resultado) -> Result<()> {
        let copia: Resultado = serde_json::from_str(&serde_json::to_string(resultado)?)?;
        enviar(self.escritura, &Evento::Resultado(copia))
    }
}

pub fn servir(ctx: &Ctx, catalog: &Catalog) -> Result<()> {
    let ruta = ruta_socket(ctx);
    // Un socket huérfano de una ejecución anterior impediría escuchar.
    let _ = std::fs::remove_file(&ruta);

    let escucha = UnixListener::bind(&ruta)
        .with_context(|| format!("no pude escuchar en {}", ruta.display()))?;
    std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o600))?;

    println!(
        "{} {}",
        terminal::paint("syso · demonio escuchando en", terminal::BOLD),
        terminal::paint(&ruta.display().to_string(), terminal::DIM)
    );

    for conexion in escucha.incoming() {
        let flujo = match conexion {
            Ok(f) => f,
            Err(e) => {
                eprintln!("conexión rechazada: {e}");
                continue;
            }
        };
        // Se atiende una conexión cada vez, a propósito. Dos intenciones
        // mutando el mismo espacio de trabajo a la vez producirían diffs que
        // ya no describen el resultado — el mismo fallo que se arregló
        // calculando los pasos en orden, pero entre procesos.
        if let Err(e) = atender(ctx, catalog, flujo) {
            eprintln!("sesión terminada con error: {e:#}");
        }
    }
    Ok(())
}

fn atender(ctx: &Ctx, catalog: &Catalog, flujo: UnixStream) -> Result<()> {
    let mut lectura = BufReader::new(flujo.try_clone()?);
    let mut escritura = flujo;

    let Some(peticion) = recibir::<Peticion>(&mut lectura)? else {
        return Ok(());
    };

    match peticion {
        Peticion::Intencion { texto, planificador, seco } => {
            let planner = crate::pick_planner_por_nombre(planificador.as_deref())?;
            let mut con = PorSocket {
                escritura: &mut escritura,
                lectura: &mut lectura,
            };
            if let Err(e) = sesion::intencion(ctx, catalog, &texto, &*planner, seco, &mut con) {
                enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
            }
        }
        Peticion::Aprobacion(_) => {
            enviar(
                &mut escritura,
                &Evento::Error("una aprobación sin propuesta previa".into()),
            )?;
        }
        Peticion::ConsultarEstadoGit { workspace_path } => {
            match crate::git::GitAnalyzer::global().consultar_estado(Path::new(&workspace_path)) {
                Ok(Some(status)) => {
                    enviar(&mut escritura, &Evento::EstadoGit(status))?;
                }
                Ok(None) => {
                    enviar(&mut escritura, &Evento::NoEsRepoGit)?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ListarTickets { workspace_path } => {
            match crate::spec::SpecEngine::global().listar_tickets(Path::new(&workspace_path)) {
                Ok(tickets) => {
                    enviar(&mut escritura, &Evento::ListaTickets(tickets))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ObtenerTicket { workspace_path, ticket_id } => {
            match crate::spec::SpecEngine::global().obtener_ticket(Path::new(&workspace_path), &ticket_id) {
                Ok(detalle) => {
                    enviar(&mut escritura, &Evento::DetalleTicket(detalle))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::DiagnosticarPuertos { port } => {
            match crate::net::diagnosticar_puertos(port) {
                Ok(puertos) => {
                    enviar(&mut escritura, &Evento::EstadoPuertos(puertos))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::IniciarFlow { workspace_path, ticket_id } => {
            match crate::flow::FlowEngine::global().iniciar_tarea(
                Path::new(&workspace_path),
                &ctx.state,
                &ticket_id,
            ) {
                Ok(task) => {
                    enviar(&mut escritura, &Evento::EstadoFlow(Some(task)))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ConsultarFlow { ticket_id } => {
            let task = crate::flow::FlowEngine::global().consultar_tarea(&ticket_id);
            enviar(&mut escritura, &Evento::EstadoFlow(task))?;
        }
        Peticion::ListarFlows { .. } => {
            let tasks = crate::flow::FlowEngine::global().listar_tareas();
            enviar(&mut escritura, &Evento::ListaFlows(tasks))?;
        }
        Peticion::AprobarFlow { ticket_id, decision } => {
            match crate::flow::FlowEngine::global().aprobar_tarea(&ticket_id, decision) {
                Ok(task) => {
                    enviar(&mut escritura, &Evento::EstadoFlow(Some(task)))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------------- lado cliente

pub fn hay_demonio(ctx: &Ctx) -> bool {
    let ruta = ruta_socket(ctx);
    ruta.exists() && UnixStream::connect(&ruta).is_ok()
}

/// Manda una intención al demonio y dibuja lo que conteste.
///
/// Fíjate en que el dibujado es EL MISMO `Terminal` que usa el modo local:
/// no hay dos maneras de enseñar un plan, y por eso no pueden divergir.
pub fn intencion_remota(
    ruta: &Path,
    texto: &str,
    planificador: Option<&str>,
    seco: bool,
    asumir_si: bool,
) -> Result<()> {
    let flujo = UnixStream::connect(ruta)
        .with_context(|| format!("no pude conectar con el demonio en {}", ruta.display()))?;
    let mut lectura = BufReader::new(flujo.try_clone()?);
    let mut escritura = flujo;

    enviar(
        &mut escritura,
        &Peticion::Intencion {
            texto: texto.to_string(),
            planificador: planificador.map(str::to_string),
            seco,
        },
    )?;

    let mut pantalla = terminal::Terminal::new(asumir_si);

    while let Some(evento) = recibir::<Evento>(&mut lectura)? {
        match evento {
            Evento::Inicio { intencion, planificador } => {
                pantalla.inicio(&intencion, &planificador)?
            }
            Evento::Nota(t) => pantalla.nota(&t)?,
            Evento::Propuesta(p) => {
                let decision = pantalla.propone(&p)?;
                enviar(&mut escritura, &Peticion::Aprobacion(decision))?;
            }
            Evento::Salida(t) => pantalla.salida(&t)?,
            Evento::Resultado(r) => pantalla.resultado(&r)?,
            Evento::EstadoGit(status) => {
                pantalla.nota(&format!("git branch: {:?}", status.rama))?;
            }
            Evento::NoEsRepoGit => {
                pantalla.nota("no es un repositorio Git")?;
            }
            Evento::ListaTickets(tickets) => {
                pantalla.nota(&format!("tickets disponibles: {}", tickets.len()))?;
            }
            Evento::DetalleTicket(detalle) => {
                if let Some(t) = detalle {
                    pantalla.nota(&format!("ticket {}: {}", t.id, t.titulo))?;
                }
            }
            Evento::EstadoPuertos(puertos) => {
                pantalla.nota(&format!("puertos en escucha: {}", puertos.len()))?;
            }
            Evento::EstadoFlow(task) => {
                if let Some(t) = task {
                    pantalla.nota(&format!("antFlow {}: {}", t.ticket_id, t.estado.etiqueta()))?;
                }
            }
            Evento::ListaFlows(tasks) => {
                pantalla.nota(&format!("tareas antFlow activas: {}", tasks.len()))?;
            }
            Evento::TransicionFlow { ticket_id, estado_nuevo, detalle, .. } => {
                pantalla.nota(&format!("[antFlow {ticket_id}] ➔ {}: {detalle}", estado_nuevo.etiqueta()))?;
            }
            Evento::Error(m) => bail!("{m}"),
        }
    }
    Ok(())
}
