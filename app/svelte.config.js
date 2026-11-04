import adapter from '@sveltejs/adapter-static';

/** The Tasks UI (src/lib, src/routes/tasks) plus the app's shell (src/app), which
 *  installs the Tauri TasksClient. */
/** @type {import('@sveltejs/kit').Config} */
const config = {
	compilerOptions: {
		runes: ({ filename }) => (filename.split(/[/\\]/).includes('node_modules') ? undefined : true),
	},
	kit: {
		// A single-page app inside the webview; index.html is the fallback for every route.
		adapter: adapter({ pages: 'build', assets: 'build', fallback: 'index.html', strict: false }),
		alias: {
			$shell: 'src/app',
		},
	},
};

export default config;
