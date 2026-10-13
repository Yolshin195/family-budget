/* Ранний скрипт (до отрисовки): отметка «JS включён» и состояние бокового меню — без мигания.
   Меню: выбор пользователя из localStorage; по умолчанию на 768–1279 px свёрнуто (ТЗ, раздел 9). */
(function () {
  var d = document.documentElement;
  d.classList.add('js');
  var w = window.innerWidth, s = null;
  try { s = localStorage.getItem('fb.sidebar'); } catch (e) { /* приватный режим */ }
  d.setAttribute('data-sidebar', s || (w >= 768 && w < 1280 ? 'collapsed' : 'expanded'));
})();
