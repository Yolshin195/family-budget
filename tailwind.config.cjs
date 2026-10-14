/** Tailwind для шаблонов Askama всех фич. Классы ищутся в шаблонах и в Rust-коде (HTML вне шаблонов).
 *
 * Дизайн-система (ТЗ, раздел 9): цвета — токены-переменные из styles/app.css (светлая и тёмная тема),
 * нейтральная шкала + один акцент + семантика (доход, просрочка/превышение, ≥ 80 % лимита).
 * Сетка 4 px, кегли 12/14/16/20/24/30, радиусы 6/8/12. */
const token = (name) => `rgb(var(--${name}) / <alpha-value>)`;

module.exports = {
  content: ['crates/**/templates/**/*.html', 'crates/app/static/app.js'],
  darkMode: ['class', '[data-theme="dark"]'],
  theme: {
    fontSize: {
      xs: ['0.75rem', { lineHeight: '1rem' }],
      sm: ['0.875rem', { lineHeight: '1.25rem' }],
      base: ['1rem', { lineHeight: '1.5rem' }],
      xl: ['1.25rem', { lineHeight: '1.75rem' }],
      '2xl': ['1.5rem', { lineHeight: '2rem' }],
      '3xl': ['1.875rem', { lineHeight: '2.25rem' }],
    },
    borderRadius: { none: '0', sm: '4px', md: '6px', DEFAULT: '8px', lg: '8px', xl: '12px', full: '9999px' },
    extend: {
      fontFamily: { sans: ['Inter', 'Noto Sans Thai', 'system-ui', '-apple-system', 'Segoe UI', 'sans-serif'] },
      colors: {
        canvas: token('canvas'),
        surface: token('surface'),
        sunken: token('sunken'),
        line: token('line'),
        'line-strong': token('line-strong'),
        default: token('text'),
        muted: token('text-muted'),
        subtle: token('text-subtle'),
        accent: { DEFAULT: token('accent'), hover: token('accent-hover'), soft: token('accent-soft'), text: token('accent-text') },
        income: { DEFAULT: token('income'), soft: token('income-soft') },
        danger: { DEFAULT: token('danger'), soft: token('danger-soft') },
        warn: { DEFAULT: token('warn'), soft: token('warn-soft') },
        info: { DEFAULT: token('info'), soft: token('info-soft') },
      },
      maxWidth: { content: '1280px' },
    },
  },
  // Без переменных прозрачности (--tw-*-opacity): CSS вдвое компактнее; прозрачность — через `/50`.
  corePlugins: { backgroundOpacity: false, textOpacity: false, borderOpacity: false, divideOpacity: false, placeholderOpacity: false, ringOpacity: false },
  plugins: [],
};
