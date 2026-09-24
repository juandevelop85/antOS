import type { Metadata, Viewport } from "next";
import { JetBrains_Mono, Manrope, Outfit } from "next/font/google";
import "./globals.css";

const display = Outfit({ subsets: ["latin"], weight: ["500", "600", "700"], variable: "--font-display" });
const sans = Manrope({ subsets: ["latin"], weight: ["400", "500", "600", "700"], variable: "--font-sans" });
const mono = JetBrains_Mono({ subsets: ["latin"], weight: ["400", "500", "700"], variable: "--font-mono" });

// En Vercel, las URLs absolutas de Open Graph salen del dominio de producción.
const siteUrl = process.env.VERCEL_PROJECT_PRODUCTION_URL
  ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
  : "http://localhost:3000";

const description =
  "antOS: sistema operativo personal para desarrolladores con orquestación multi-agente nativa, capacidades tipadas, sandbox del kernel y reversión atómica.";

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: "antOS · El sistema operativo para desarrolladores",
  description,
  openGraph: {
    title: "antOS",
    description,
    images: [{ url: "/assets/wallpaper.webp", width: 1672, height: 941, alt: "La hormiga de antOS sobre las colinas al anochecer" }],
    locale: "es_ES",
    type: "website",
  },
  twitter: { card: "summary_large_image", title: "antOS", description, images: ["/assets/wallpaper.webp"] },
};

export const viewport: Viewport = {
  themeColor: "#070b24",
  viewportFit: "cover",
};

// Aplica el tema guardado antes del primer pintado para evitar el parpadeo.
const themeScript = `try{var t=localStorage.getItem("antos-theme");if(t)document.documentElement.dataset.theme=t}catch(e){}`;

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="es" className={`${display.variable} ${sans.variable} ${mono.variable}`} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body>{children}</body>
    </html>
  );
}
