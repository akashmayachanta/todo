import adapter from '@sveltejs/adapter-static';

/** The Tasks UI (src/lib, src/routes/tasks) plus the app's shell (src/app), which
 *  installs the Tauri TasksClient. */
/** TASKS_WEB_BASE (set by `npm run build:web`) builds the browser copy that tasks-server
 *  serves under that path, into build-web/. The Tauri builds leave it unset. */
const webBase = process.env.TASKS_WEB_BASE ?? '';
const out = webBase ? 'build-web' : 'build';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	compilerOptions: {
		runes: ({ filename }) => (filename.split(/[/\\]/).includes('node_modules') ? undefined : true),
	},
	kit: {
		// A single-page app inside the webview; index.html is the fallback for every route.
		adapter: adapter({ pages: out, assets: out, fallback: 'index.html', strict: false }),
		paths: { base: webBase },
		alias: {
			$shell: 'src/app',
		},
	},
};

export default config;
