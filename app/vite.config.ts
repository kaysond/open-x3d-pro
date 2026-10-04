import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Settings follow the Tauri v2 Vite guide; array target avoids needing process.env typings.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: ['chrome105', 'safari13'],
  },
});
