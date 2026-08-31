//! Captura de voz y transcripción local.
//!
//! La voz no es una arquitectura distinta: es otra forma de escribir la misma
//! cadena de texto. Una vez transcrita, sigue exactamente el mismo recorrido
//! que si la hubieras tecleado — plan, radio de impacto, nivel de permiso,
//! diff y confirmación.
//!
//! Eso es deliberado y es lo que hace la voz aceptable. Hablarle a una
//! máquina que muta tu sistema solo tiene sentido cuando equivocarse ya es
//! barato, y por eso M3 va después del recinto y del deshacer, no antes.
//!
//! ## Por qué en local
//!
//! Mandar el audio a un servicio de transcripción tiraría por tierra media
//! premisa del proyecto: el micrófono de tu máquina de trabajo capta mucho
//! más que la frase que querías decir. La transcripción corre entera aquí.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whisper trabaja con audio de 16 kHz, mono, PCM de 16 bits. Cualquier otra
/// cosa hay que convertirla antes.
const SAMPLE_RATE: &str = "16000";

pub struct Voz {
    whisper: PathBuf,
    modelo: PathBuf,
    idioma: String,
}

impl Voz {
    pub fn discover() -> Result<Self> {
        let whisper = match std::env::var_os("SYSO_WHISPER") {
            Some(path) => PathBuf::from(path),
            None => buscar_en_path(&["whisper-cli", "whisper-cpp"]).ok_or_else(|| {
                anyhow::anyhow!(
                    "no encuentro whisper. Instálalo con:\n  brew install whisper-cpp"
                )
            })?,
        };

        let modelo = match std::env::var_os("SYSO_MODELO_VOZ") {
            Some(path) => PathBuf::from(path),
            None => modelo_por_defecto(),
        };
        if !modelo.exists() {
            bail!(
                "no encuentro el modelo de voz en {}.\n\
                 Descárgalo con:\n  ./system/instalar-voz.sh",
                modelo.display()
            );
        }

        Ok(Voz {
            whisper,
            modelo,
            idioma: std::env::var("SYSO_IDIOMA_VOZ").unwrap_or_else(|_| "es".into()),
        })
    }

    /// Graba del micrófono durante `segundos`.
    ///
    /// La primera vez, macOS pedirá permiso de micrófono al terminal desde el
    /// que se lance. Ese permiso lo concede una persona, no este programa.
    pub fn grabar(&self, segundos: u32, destino: &Path) -> Result<()> {
        let ffmpeg = buscar_en_path(&["ffmpeg"]).ok_or_else(|| {
            anyhow::anyhow!("no encuentro ffmpeg. Instálalo con:\n  brew install ffmpeg")
        })?;

        let salida = Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            // avfoundation es la capa de captura de macOS; ":default" toma la
            // entrada de audio predeterminada y ningún vídeo.
            .args(["-f", "avfoundation", "-i", ":default"])
            .args(["-t", &segundos.to_string()])
            .args(["-ar", SAMPLE_RATE, "-ac", "1", "-c:a", "pcm_s16le"])
            .arg("-y")
            .arg(destino)
            .output()
            .context("no pude lanzar ffmpeg")?;

        if !salida.status.success() {
            let detalle = String::from_utf8_lossy(&salida.stderr);
            bail!(
                "la grabación falló.\n{}\n\
                 Si es un problema de permisos, macOS tiene que autorizar el \
                 micrófono para tu terminal en Ajustes › Privacidad.",
                detalle.trim()
            );
        }
        Ok(())
    }

    /// Convierte cualquier audio al formato que espera Whisper.
    pub fn normalizar(&self, origen: &Path, destino: &Path) -> Result<()> {
        let ffmpeg = buscar_en_path(&["ffmpeg"])
            .ok_or_else(|| anyhow::anyhow!("no encuentro ffmpeg para convertir el audio"))?;

        let salida = Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(origen)
            .args(["-ar", SAMPLE_RATE, "-ac", "1", "-c:a", "pcm_s16le"])
            .arg("-y")
            .arg(destino)
            .output()
            .context("no pude lanzar ffmpeg")?;

        if !salida.status.success() {
            bail!(
                "no pude convertir el audio: {}",
                String::from_utf8_lossy(&salida.stderr).trim()
            );
        }
        Ok(())
    }

    /// `vocabulario` ceba al modelo con las palabras del dominio.
    ///
    /// Whisper decide entre candidatos parecidos usando su propio modelo de
    /// lenguaje, y "rust" no es una palabra frecuente en español: sin ayuda
    /// la transcribe como "rastre". Darle de antemano las palabras que este
    /// sistema entiende cambia esa apuesta — y esas palabras las conoce el
    /// catálogo de capacidades, así que no hay que inventarlas.
    pub fn transcribir(&self, wav: &Path, vocabulario: &str) -> Result<String> {
        let salida = Command::new(&self.whisper)
            .args(["-m"])
            .arg(&self.modelo)
            .args(["-f"])
            .arg(wav)
            .args(["-l", &self.idioma])
            // Sin marcas de tiempo y sin ruido de diagnóstico: lo único que
            // queremos por la salida estándar es la frase.
            .args(["--no-timestamps", "--no-prints"])
            .args(["--prompt", vocabulario])
            .output()
            .context("no pude lanzar whisper")?;

        if !salida.status.success() {
            bail!(
                "la transcripción falló: {}",
                String::from_utf8_lossy(&salida.stderr).trim()
            );
        }

        let texto = String::from_utf8_lossy(&salida.stdout);
        let limpio = limpiar(&texto);

        if limpio.is_empty() {
            bail!("no he entendido nada. ¿Estaba el micrófono activo?");
        }
        Ok(limpio)
    }

    pub fn modelo(&self) -> &Path {
        &self.modelo
    }
}

/// Whisper marca los silencios y ruidos con corchetes o paréntesis
/// —`[BLANK_AUDIO]`, `(música de fondo)`— y eso no es una intención.
fn limpiar(texto: &str) -> String {
    texto
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !(line.starts_with('[') || line.starts_with('(')))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn buscar_en_path(nombres: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for nombre in nombres {
            let candidato = dir.join(nombre);
            if candidato.is_file() {
                return Some(candidato);
            }
        }
    }
    None
}

fn modelo_por_defecto() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".cache/syso/modelos/ggml-base.bin")
}
