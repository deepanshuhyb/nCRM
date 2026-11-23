import type { Metadata, Viewport } from "next";
import localFont from "next/font/local";
import { cookies, headers } from "next/headers";

import "./globals.css";

// Self-hosted so the CSP can stay `font-src 'self'` and no request goes to a
// third party. Licenses sit next to the files (SIL Open Font License).
const display = localFont({
  src: "./fonts/unbounded.woff2",
  variable: "--font-unbounded",
  weight: "200 900",
  display: "swap",
});
const body = localFont({
  src: "./fonts/figtree.woff2",
  variable: "--font-figtree",
  weight: "300 900",
  display: "swap",
});

export const metadata: Metadata = {
  title: { default: "ncrm", template: "%s | ncrm" },
  description: "Run your studio: students, classes, attendance and fees.",
  robots: { index: false, follow: false },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
};

export default async function RootLayout({ children }: LayoutProps<"/">) {
  // Reading request headers makes every page render per request, which the
  // CSP nonce needs (Next.js applies it to its scripts automatically).
  await headers();
  const theme = (await cookies()).get("ncrm_theme")?.value === "emerald" ? "emerald" : "daylight";

  return (
    <html lang="en" data-theme={theme} className={`${display.variable} ${body.variable}`}>
      <body>
        <div className="counts" aria-hidden="true">
          <span className="c5">5</span>
          <span className="c6">6</span>
          <span className="c7">7</span>
          <span className="c8">8</span>
        </div>
        <div className="page-layer">{children}</div>
      </body>
    </html>
  );
}
