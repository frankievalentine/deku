import path from "node:path";
import node from "@astrojs/node";
import react from "@astrojs/react";
import { auditLogPlugin } from "@emdash-cms/plugin-audit-log";
import { defineConfig } from "astro/config";
import emdash, { local } from "emdash/astro";
import { sqlite } from "emdash/db";

const dataDir = path.resolve(process.env.EMDASH_DATA_DIR ?? ".");
const dbPath = process.env.EMDASH_DB_PATH ?? path.join(dataDir, "data.db");
const uploadsDir =
	process.env.EMDASH_UPLOADS_DIR ?? path.join(dataDir, "uploads");
const dbUrl = dbPath === ":memory:" ? dbPath : `file:${dbPath}`;

export default defineConfig({
	output: "server",
	adapter: node({
		mode: "standalone",
	}),
	// Example: allowed domains for reverse proxy
	// security: {
	// 	allowedDomains: [
	// 		{ hostname: "emdash.local", protocol: "http" },
	// 		{ hostname: "emdash.local", protocol: "https" },
	// 	],
	// },
	image: {
		layout: "constrained",
		responsiveStyles: true,
	},
	integrations: [
		react(),
		emdash({
			database: sqlite({ url: dbUrl }),
			storage: local({
				directory: uploadsDir,
				baseUrl: "/_emdash/api/media/file",
			}),
			plugins: [auditLogPlugin()],
			// HTTPS reverse proxy: uncomment so passkey verify matches browser origin
			// passkeyPublicOrigin: "https://emdash.local:8443",
		}),
	],
	devToolbar: { enabled: false },
	// Example: allowed hosts for reverse proxy
	// vite: {
	// 	server: {
	// 		allowedHosts: ["emdash.local"],
	// 	},
	// },
});
