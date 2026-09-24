"use client";

export default function ThemeToggle() {
  const toggle = () => {
    const root = document.documentElement;
    const current =
      root.dataset.theme ?? (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
    const next = current === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    try {
      localStorage.setItem("antos-theme", next);
    } catch {
      // Sin almacenamiento: el cambio vale solo para esta visita.
    }
  };

  return (
    <button className="icon-btn" type="button" aria-label="Cambiar tema" onClick={toggle}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9z" />
      </svg>
    </button>
  );
}
