import Image from "next/image";
import type { CSSProperties } from "react";
import DesktopDemo from "@/components/DesktopDemo";
import ThemeToggle from "@/components/ThemeToggle";
import {
  AGENT_CARDS,
  COMPARISON,
  CRATES,
  FLOW,
  PROBLEMS,
  RELEASES_URL,
  REPO_URL,
  STATUS,
  STATUS_LABEL,
  TICKETS,
  docUrl,
  type StatusKind,
} from "@/lib/content";

const Tag = ({ kind }: { kind: StatusKind }) => <span className={`tag tag-${kind}`}>{STATUS_LABEL[kind]}</span>;

export default function Home() {
  const progress = ((TICKETS.done / TICKETS.total) * 100).toFixed(1);

  return (
    <>
      <header className="nav">
        <div className="wrap nav-inner">
          <a className="brand" href="#top" aria-label="antOS, inicio">
            <Image src="/assets/icon.webp" alt="" width={34} height={34} priority />
            <span className="wordmark">ant<b>OS</b></span>
          </a>
          <nav className="nav-links" aria-label="Secciones">
            <a href="#como">Cómo funciona</a>
            <a href="#agentes">Agentes</a>
            <a href="#seguridad">Seguridad</a>
            <a href="#estado">Estado</a>
          </nav>
          <div className="nav-actions">
            <ThemeToggle />
            <a className="btn btn-primary btn-sm" href="#descarga">Descargar</a>
          </div>
        </div>
      </header>

      <main id="top">
        {/* Hero: el wallpaper del sistema */}
        <section className="hero">
          <div className="hero-bg" aria-hidden="true" />
          <div className="wrap">
            <div className="hero-copy">
              <p className="eyebrow"><span className="dot" /> v0.2.0 · prerelease · x86_64 y AArch64</p>
              <h1>La IA no ejecuta comandos <em>a ciegas</em>.</h1>
              <p className="lede">
                antOS es un sistema operativo para desarrolladores. Un equipo de agentes trabaja en{" "}
                <strong>worktrees aislados</strong> y con <strong>capacidades tipadas</strong> dentro de un{" "}
                <strong>recinto del kernel</strong>. Cada cambio se puede deshacer.
              </p>
              <div className="cta-row">
                <a className="btn btn-primary" href="#descarga">Descargar la ISO</a>
                <a className="btn btn-ghost" href={REPO_URL}>Ver en GitHub →</a>
              </div>
              <p className="hint">
                Prueba la barra de intenciones del escritorio que tienes justo debajo. Es una demostración
                guionizada, no un sistema en vivo.
              </p>
            </div>
          </div>
        </section>

        {/* Demo: el escritorio sobre las colinas */}
        <section className="demo" aria-label="Demostración interactiva">
          <div className="wrap">
            <DesktopDemo />
          </div>
        </section>

        <section className="section">
          <div className="wrap">
            <p className="kicker">El problema</p>
            <h2>Los sistemas operativos de hoy son de otra época.</h2>
            <div className="problem-grid">
              {PROBLEMS.map((p) => (
                <article className="problem" key={p.title}>
                  <h3>{p.title}</h3>
                  <p>{p.body}</p>
                </article>
              ))}
            </div>
          </div>
        </section>

        <section className="section section-alt" id="como">
          <div className="wrap">
            <p className="kicker">Cómo funciona</p>
            <h2>Tres capas, un contrato tipado entre ellas.</h2>
            <div className="layers">
              <div className="layer">
                <div className="layer-tag">Superficie</div>
                <div>
                  <h3>Escritorio Wayland · GTK4</h3>
                  <p>
                    Barra de intenciones, tablero Kanban de agentes (<kbd>Super</kbd>+<kbd>A</kbd>), visor de diffs,
                    terminal VTE y bandeja de aprobaciones.
                  </p>
                </div>
              </div>
              <div className="layer-link">IPC tipado · <code>antos-protocolo</code></div>
              <div className="layer">
                <div className="layer-tag">Contexto</div>
                <div>
                  <h3>Motor multi-agente y de contexto</h3>
                  <p>
                    Orquestador antFlow, parser de tickets Markdown, memoria semántica en SQLite, LLM local con Ollama
                    o remoto, entornos Nix/Devbox y servicios efímeros.
                  </p>
                </div>
              </div>
              <div className="layer-link">Capacidades tipadas · sandbox</div>
              <div className="layer layer-core">
                <div className="layer-tag">Núcleo</div>
                <div>
                  <h3><code>antosd</code>: ejecución aislada</h3>
                  <p>
                    Cálculo del blast radius, recinto Landlock (Linux) o Seatbelt (macOS), cuotas de recursos,
                    bitácora inmutable e instantáneas atómicas.
                  </p>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section className="section" id="agentes">
          <div className="wrap">
            <p className="kicker">antFlow</p>
            <h2>Una colonia de agentes, cada uno con su oficio.</h2>
            <p className="section-lede">
              Trabajan como demonios del sistema sobre <em>Git worktrees</em> efímeros. Nunca tocan tu rama directamente.
            </p>
            <div className="agents">
              {AGENT_CARDS.map((a) => (
                <article className="agent" key={a.name} style={{ "--c": a.color } as CSSProperties}>
                  <div className="agent-icon">{a.icon}</div>
                  <h3>{a.name}</h3>
                  <p>{a.body}</p>
                  <code>{a.cmd}</code>
                </article>
              ))}
            </div>
          </div>
        </section>

        <section className="section section-alt" id="seguridad">
          <div className="wrap">
            <p className="kicker">Seguridad y reversibilidad</p>
            <h2>Del «ejecuta esto» al «esto es lo que va a pasar».</h2>
            <ol className="flow">
              {FLOW.map((step, i) => (
                <li key={step.title}>
                  <span>{i + 1}</span>
                  <div>
                    <h3>{step.title}</h3>
                    <p>{step.body}</p>
                  </div>
                </li>
              ))}
            </ol>

            <div className="split">
              <div className="term">
                <div className="term-bar"><i /><i /><i /><span>zsh — antos</span></div>
                <pre>
                  <span className="p">$</span> antos grant secret.GITHUB_TOKEN --minutos 15 \{"\n"}
                  {"    "}--para &quot;sincronizar releases&quot;{"\n"}
                  <span className="ok">✓</span> concesión g-7f2a válida hasta 09:56{"\n"}
                  <span className="p">$</span> antos undo --ticket T3.2{"\n"}
                  <span className="warn">↺</span> restaurando instantánea previa a T3.2{"\n"}
                  <span className="ok">✓</span> 4 ficheros revertidos · worktree eliminado
                </pre>
              </div>
              <div className="table-wrap">
                <table className="compare">
                  <caption className="sr-only">Comparativa entre un SO convencional y antOS</caption>
                  <thead>
                    <tr><th /><th>SO convencional</th><th>antOS</th></tr>
                  </thead>
                  <tbody>
                    {COMPARISON.map(([row, before, after]) => (
                      <tr key={row}><th>{row}</th><td>{before}</td><td>{after}</td></tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </div>
        </section>

        <section className="section">
          <div className="wrap">
            <p className="kicker">Hecho en Rust, de arriba abajo</p>
            <h2>Desde la barra de escritorio hasta el kernel <code>no_std</code>.</h2>
            <div className="stack">
              {CRATES.map(([name, body]) => (
                <div key={name}><strong>{name}</strong><span>{body}</span></div>
              ))}
            </div>
          </div>
        </section>

        <section className="section section-alt" id="estado">
          <div className="wrap">
            <p className="kicker">Estado honesto</p>
            <h2>Lo que ya es real y lo que todavía no.</h2>
            <p className="section-lede">
              Esta página aplica la misma regla que el código (T31.14): ninguna simulación se presenta como garantía.
            </p>
            <div className="legend">
              {(Object.keys(STATUS_LABEL) as StatusKind[]).map((k) => <Tag key={k} kind={k} />)}
            </div>
            <ul className="status">
              {STATUS.map((s) => (
                <li key={s.title}>
                  <Tag kind={s.kind} />
                  <div><strong>{s.title}</strong><p>{s.body}</p></div>
                </li>
              ))}
            </ul>
            <div className="progress" role="img" aria-label={`${TICKETS.done} de ${TICKETS.total} tickets completados`}>
              <div className="progress-bar" style={{ "--p": `${progress}%` } as CSSProperties} />
              <span>
                <strong>{TICKETS.done} / {TICKETS.total}</strong> tickets completados ·{" "}
                <a href={docUrl("docs/tickets/README.md")}>ver backlog</a>
              </span>
            </div>
          </div>
        </section>

        <section className="section download" id="descarga">
          <div className="wrap download-inner">
            <Image
              className="mascot"
              src="/assets/mascot.webp"
              alt="La hormiga de antOS asomando sobre una colina"
              width={405}
              height={375}
            />
            <h2>Pruébalo en una máquina virtual esta tarde.</h2>
            <p className="section-lede">
              Descarga la ISO, verifica <code>SHA256SUMS</code> y arranca directamente en el escritorio en vivo. Cuando
              quieras instalarlo en disco, ejecuta <code>antos install</code>.
            </p>
            <div className="dl-cards">
              <a className="dl-card" href={RELEASES_URL}>
                <strong>AArch64</strong><span>v0.2.0 · prerelease</span><em>Apple Silicon (UTM) · ARM64</em>
              </a>
              <a className="dl-card" href={RELEASES_URL}>
                <strong>x86_64</strong><span>ver releases</span><em>BIOS / UEFI · VirtualBox · QEMU</em>
              </a>
            </div>
            <p className="small">
              <a href={docUrl("docs/guia-emulacion-utm-virtualbox.md")}>Guía UTM y VirtualBox</a> ·{" "}
              <a href={docUrl("docs/guia-live-usb-e-instalacion-fisica.md")}>Live USB e instalación física</a> ·{" "}
              <a href={docUrl("docs/manual-de-comandos.md")}>Manual de comandos</a>
            </p>
          </div>
        </section>
      </main>

      <footer className="footer">
        <div className="wrap footer-inner">
          <span className="footer-brand">
            <Image src="/assets/icon.webp" alt="" width={22} height={22} /> antOS · hecho en Rust
          </span>
          <a href={REPO_URL}>github.com/juandevelop85/antOS</a>
        </div>
      </footer>
    </>
  );
}
