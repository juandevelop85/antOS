//! La frontera entre el recorrido y quien lo mira.
//!
//! Hasta ahora el recorrido imprimía por pantalla y preguntaba por stdin, así
//! que solo podía tener un tipo de interlocutor. Estas estructuras son lo que
//! el recorrido produce, sin decidir cómo se muestra — y por eso valen igual
//! para el terminal, para un socket y, mañana, para una superficie gráfica.
//!
//! Que además sean serializables no es casualidad: el protocolo del demonio
//! ES esto. Un cliente gráfico recibirá exactamente lo mismo que el terminal
//! imprime, y no habrá dos verdades que mantener sincronizadas.


#[allow(unused_imports)]
pub use antos_protocol::{BlastRadius, Enclosure, ExecutionResult, Proposal};
#[allow(unused_imports)]
pub use antos_protocol::{Propuesta, Radio, Recinto, Resultado};

/// Con quién habla el recorrido.
///
/// El terminal lo implementa imprimiendo y leyendo stdin; el demonio,
/// escribiendo y leyendo el socket. El recorrido no sabe cuál es cuál, y esa
/// ignorancia es justo lo que permite que haya más de una interfaz sin
/// duplicar la lógica ni las garantías.
pub trait Interlocutor {
    /// Qué se ha pedido y quién va a planificarlo.
    fn inicio(&mut self, intencion: &str, planificador: &str) -> anyhow::Result<()>;

    fn nota(&mut self, texto: &str) -> anyhow::Result<()>;

    /// Enseña la propuesta y devuelve si se aprueba.
    ///
    /// Enseñar y decidir son el mismo acto, por eso van en el mismo método:
    /// separarlos permitiría una interfaz que decide sin haber enseñado.
    fn propone(&mut self, propuesta: &Propuesta) -> anyhow::Result<bool>;

    /// Lo que una capacidad de lectura haya producido.
    fn salida(&mut self, texto: &str) -> anyhow::Result<()>;

    fn resultado(&mut self, resultado: &Resultado) -> anyhow::Result<()>;
}
