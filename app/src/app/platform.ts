// Inside the Tauri app (Windows or Android), or a plain browser on tasks-server's
// /tasks-app page (webClient.ts), which talks straight to the server's REST API.

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
