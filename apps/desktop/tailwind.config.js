/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        'brand-primary': '#4ade80', // Green accent as requested in vision
        'bg-base': '#0f172a', // slate-900
        'bg-surface': '#1e293b', // slate-800
        background: '#0f172a',
        foreground: '#f8fafc',
        primary: {
          DEFAULT: '#3b82f6',
          foreground: '#ffffff',
        },
        secondary: {
          DEFAULT: '#334155',
          foreground: '#f8fafc',
        },
        muted: {
          DEFAULT: '#1e293b',
          foreground: '#94a3b8',
        },
        destructive: {
          DEFAULT: '#ef4444',
          foreground: '#ffffff',
        },
        border: '#334155',
        input: '#1e293b',
        ring: '#3b82f6',
      }
    },
  },
  plugins: [],
}
