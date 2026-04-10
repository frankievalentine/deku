import "./globals.css";
import type { ReactNode } from "react";

export const metadata = {
  title: "Deku Next.js Starter",
  description: "A simple Next.js starter template for Deku."
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
