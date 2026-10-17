/** Tailwind для сайта: дизайн-система приложения (../tailwind.config.cjs), классы — из шаблонов и Markdown сайта.
 * Классы, которые шаблоны собирают из данных (kind-releases, status-planned, change-new, note-tip), в тексте не встречаются
 * целиком — поэтому safelist. */
const base = require('../tailwind.config.cjs');

module.exports = {
  ...base,
  content: ['site/templates/**/*.html', 'site/content/**/*.md'],
  safelist: [{ pattern: /^(kind|status|change|note)-/ }],
};
