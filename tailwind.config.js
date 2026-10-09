/** Tailwind для шаблонов Askama всех фич. Классы ищутся в шаблонах и в Rust-коде (HTML вне шаблонов). */
module.exports = {
  content: ['crates/**/templates/**/*.html', 'crates/**/src/**/*.rs'],
  theme: {
    extend: {
      fontFamily: { sans: ['Inter', 'Noto Sans Thai', 'system-ui', 'sans-serif'] },
      colors: {
        brand: { 50: '#ecfdf5', 100: '#d1fae5', 500: '#10b981', 600: '#059669', 700: '#047857', 900: '#064e3b' },
      },
    },
  },
  plugins: [],
};
