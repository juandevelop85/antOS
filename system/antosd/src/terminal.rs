//! El interlocutor de terminal: imprime y pregunta por stdin.
//!
//! Es una de las implementaciones posibles de `Interlocutor`, no la única ni
//! la privilegiada. Todo lo que sabe hacer es dibujar lo que el recorrido le
//! entrega y devolver una decisión.

use crate::capability::Tier;
use crate::preview::Line;
use crate::protocolo::{Interlocutor, Propuesta, Resultado};
use anyhow::Result;
use std::io::Write;

pub const BOLD: &str = "1";
pub const DIM: &str = "2";
pub const RED: &str = "31";
pub const GREEN: &str = "32";
pub const YELLOW: &str = "33";
pub const BLUE: &str = "34";
pub const CYAN: &str = "36";

pub fn tier_color(tier: Tier) -> &'static str {
    match tier {
        Tier::Auto => GREEN,
        Tier::Confirm => YELLOW,
        Tier::Grant => RED,
    }
}

pub fn paint(text: &str, code: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() {
        return text.to_string();
    }
    format!("\x1b[{code}m{text}\x1b[0m")
}

pub fn ellipsis(s: &str, max: usize) -> String {
    let flat = s.replace('\n', "⏎");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max - 1).collect::<String>())
    }
}

pub struct Terminal {
    /// Responder que sí sin preguntar. Sigue pasando por `propone`, así que
    /// la propuesta se enseña igual: lo que cambia es quién contesta, no si
    /// hubo puerta.
    pub asumir_si: bool,
}

impl Terminal {
    pub fn new(asumir_si: bool) -> Self {
        Terminal { asumir_si }
    }
}

impl Interlocutor for Terminal {
    fn inicio(&mut self, intencion: &str, planificador: &str) -> Result<()> {
        println!();
        println!("{}", paint("antOS", BOLD));
        println!("  {}", paint(&format!("«{intencion}»"), DIM));
        println!("  {}", paint(&format!("planificador: {planificador}"), DIM));
        Ok(())
    }

    fn nota(&mut self, texto: &str) -> Result<()> {
        println!("  {}", paint(&format!("dice: {texto}"), DIM));
        Ok(())
    }

    fn propone(&mut self, propuesta: &Propuesta) -> Result<bool> {
        println!();
        println!("{}", paint("plan", BOLD));
        for (i, step) in propuesta.plan.steps.iter().enumerate() {
            let args = step
                .args
                .iter()
                .map(|(k, v)| format!("{k}={}", ellipsis(v, 40)))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  {}. {}  {}", i + 1, step.capability, paint(&args, DIM));
        }

        println!();
        println!("{}", paint("cambios", BOLD));
        for line in &propuesta.cambios {
            match line {
                Line::Info(t) => println!("  {t}"),
                Line::Add(t) => println!("  {}", paint(&format!("+{t}"), GREEN)),
                Line::Del(t) => println!("  {}", paint(&format!("-{t}"), RED)),
            }
        }

        println!();
        println!("{}", paint("radio de impacto", BOLD));
        let radio = &propuesta.radio;
        for (etiqueta, rutas) in [
            ("escribe ", &radio.escribe),
            ("borra   ", &radio.borra),
            ("lee     ", &radio.lee),
        ] {
            if !rutas.is_empty() {
                println!("  {etiqueta}  {}", ellipsis(&rutas.join(", "), 68));
            }
        }
        if !radio.sistema.is_empty() {
            println!(
                "  {}  {}",
                paint("SISTEMA ", RED),
                ellipsis(&radio.sistema.join(", "), 68)
            );
        }
        if !radio.red.is_empty() {
            println!("  red       {}", radio.red.join(", "));
        }
        println!(
            "  nivel     {} {}",
            paint(propuesta.nivel.label(), tier_color(propuesta.nivel)),
            paint(&format!("— {}", propuesta.razones.join("; ")), DIM)
        );
        let color_recinto = if propuesta.recinto.motor == "ninguno" { RED } else { GREEN };
        println!(
            "  recinto   {} {}",
            paint(&propuesta.recinto.motor, color_recinto),
            paint(&format!("— {}", propuesta.recinto.garantiza), DIM)
        );

        if propuesta.seco {
            println!();
            println!("{}", paint("· marcha en seco, no se ejecuta nada", DIM));
            return Ok(false);
        }

        // Nivel automático: no hay nada que preguntar.
        if propuesta.nivel == Tier::Auto {
            return Ok(true);
        }

        if self.asumir_si {
            println!();
            println!("{}", paint("· aprobado sin preguntar (--si)", DIM));
            return Ok(true);
        }

        print!("\n{} ", paint("¿ejecutar? [s/N]", BOLD));
        std::io::stdout().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Ok(false);
        }
        Ok(matches!(
            line.trim().to_lowercase().as_str(),
            "s" | "si" | "sí" | "y" | "yes"
        ))
    }

    fn salida(&mut self, texto: &str) -> Result<()> {
        println!();
        println!("{}", texto.trim_end());
        Ok(())
    }

    fn resultado(&mut self, resultado: &Resultado) -> Result<()> {
        println!();
        if !resultado.ok {
            println!("{} {}", paint("✗", RED), resultado.mensaje);
            return Ok(());
        }
        match &resultado.instantanea {
            Some(id) => println!(
                "{} {}",
                paint(&resultado.mensaje, GREEN),
                paint(&format!("· instantánea {id} · «antos undo» lo revierte"), DIM)
            ),
            None => println!("{}", paint(&resultado.mensaje, GREEN)),
        }
        Ok(())
    }
}
