import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(() => {
  return {
    build: {
      outDir: 'build',
    },
    plugins: [react()],
    test: {
        environment: "jsdom",
        include: ["**/__tests__/**/*.{js,jsx}"]
    }
  };
});
