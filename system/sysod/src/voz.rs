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

/// Por debajo de este nivel medio, se considera que no ha hablado nadie.
///
/// El número sale de medir, no de suponer. En esta máquina:
///
/// | fuente                  | nivel medio |
/// |-------------------------|-------------|
/// | silencio digital        | −91 dB      |
/// | habitación en silencio  | −47 dB      |
/// | alguien hablando        | −18 dB      |
///
/// −40 dB deja siete decibelios de margen sobre el ruido de una habitación y
/// veintidós por debajo de una voz.
const UMBRAL_DB: f32 = -40.0;

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
    pub fn grabar(&self, segundos: u32, destino: &Path, dispositivo: &str) -> Result<()> {
        let ffmpeg = buscar_en_path(&["ffmpeg"]).ok_or_else(|| {
            anyhow::anyhow!("no encuentro ffmpeg. Instálalo con:\n  brew install ffmpeg")
        })?;

        let mut orden = Command::new(&ffmpeg);
        crate::sandbox::sin_secretos(&mut orden);
        let salida = orden
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            // avfoundation es la capa de captura de macOS. Los dos puntos
            // iniciales significan "sin vídeo"; detrás va el índice del
            // dispositivo de audio.
            //
            // No vale con ":default": en una máquina con Teams, Zoom o
            // cualquier cosa que instale un dispositivo virtual, el
            // predeterminado es ESE y no el micrófono. Grabarías silencio sin
            // enterarte.
            .args(["-f", "avfoundation", "-i", dispositivo])
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

        let mut orden = Command::new(&ffmpeg);
        crate::sandbox::sin_secretos(&mut orden);
        let salida = orden
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
    /// Nivel medio del audio, en decibelios.
    fn nivel_db(&self, wav: &Path) -> Result<f32> {
        let ffmpeg = buscar_en_path(&["ffmpeg"])
            .ok_or_else(|| anyhow::anyhow!("no encuentro ffmpeg"))?;

        let mut orden = Command::new(&ffmpeg);
        crate::sandbox::sin_secretos(&mut orden);
        let salida = orden
            .args(["-hide_banner", "-nostdin", "-i"])
            .arg(wav)
            .args(["-af", "volumedetect", "-f", "null", "-"])
            .output()
            .context("no pude medir el nivel del audio")?;

        let texto = String::from_utf8_lossy(&salida.stderr);
        for linea in texto.lines() {
            if let Some((_, resto)) = linea.split_once("mean_volume:") {
                if let Some(numero) = resto.split_whitespace().next() {
                    return Ok(numero.parse::<f32>().unwrap_or(f32::NEG_INFINITY));
                }
            }
        }
        // Si no se puede medir, se deja pasar: mejor transcribir de más que
        // negarse a escuchar por un fallo de la medida.
        Ok(0.0)
    }

    pub fn transcribir(&self, wav: &Path, vocabulario: &str) -> Result<String> {
        // La puerta más importante de todo el módulo.
        //
        // Whisper ante ruido de fondo no dice "no he oído nada": se inventa
        // una frase perfectamente formada. Grabando una habitación vacía
        // salió «La gente se puede hacer un proyecto de trabajo.» — una
        // intención plausible, con la que un planificador puede construir un
        // plan de verdad.
        //
        // Filtrar los marcadores de silencio no basta, porque la alucinación
        // no viene marcada. Hay que negarse a transcribir lo que no tiene
        // energía suficiente para ser una voz.
        let umbral = std::env::var("SYSO_UMBRAL_VOZ")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(UMBRAL_DB);

        let nivel = self.nivel_db(wav)?;
        if nivel < umbral {
            bail!(
                "no he oído ninguna voz (nivel medio {nivel:.1} dB, umbral {umbral:.1} dB).\n\
                 Si estabas hablando, prueba con otro micrófono:\n  syso escucha --dispositivos"
            );
        }

        let mut orden = Command::new(&self.whisper);
        crate::sandbox::sin_secretos(&mut orden);
        let salida = orden
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

    /// Los dispositivos de audio que ve macOS, con su índice.
    pub fn dispositivos() -> Result<String> {
        let ffmpeg = buscar_en_path(&["ffmpeg"])
            .ok_or_else(|| anyhow::anyhow!("no encuentro ffmpeg"))?;

        let salida = Command::new(&ffmpeg)
            .args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""])
            .output()
            .context("no pude lanzar ffmpeg")?;

        // ffmpeg lista los dispositivos por stderr y luego "falla" a
        // propósito, porque no hay nada que abrir. No es un error.
        let texto = String::from_utf8_lossy(&salida.stderr);
        let mut lineas = Vec::new();
        let mut en_audio = false;
        for linea in texto.lines() {
            if linea.contains("audio devices") {
                en_audio = true;
                continue;
            }
            if linea.contains("video devices") {
                en_audio = false;
            }
            // Tras la lista, ffmpeg escupe su error de "no hay entrada que
            // abrir". Solo nos quedamos con las líneas que son un dispositivo.
            if en_audio {
                if let Some((_, resto)) = linea.split_once("] ") {
                    if resto.starts_with('[') {
                        lineas.push(resto.to_string());
                    }
                }
            }
        }
        Ok(lineas.join("\n"))
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
