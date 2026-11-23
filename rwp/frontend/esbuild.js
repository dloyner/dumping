const esbuild = require("esbuild");

esbuild
	.build({
		entryPoints: ["src/index.tsx"],
		bundle: true,
		outfile: "public/bundle.js",
		format: "esm",
		define: {
			"process.env.NODE_ENV": '"production"',
			__INGRESS_URL__: JSON.stringify(
				process.env.INGRESS_URL || "http://localhost:8001",
			),
		},
		minify: true,
		loader: {
			".css": "css",
			".js": "jsx",
			".ts": "ts",
			".tsx": "tsx",
			".wasm": "binary",
		},
	})
	.catch(() => process.exit(1));
