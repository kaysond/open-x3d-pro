import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Settings follow the Tauri v2 Vite guide; the target is WebView2, the only webview the app ships on.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: true,
    port: 5173,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
    // The logo lives at the repo root.
    fs: { allow: ['..'] },
  },
  build: {
    target: 'chrome105',
  },
});
