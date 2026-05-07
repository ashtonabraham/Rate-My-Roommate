(function () {
  function enhance(field) {
    var input = field.querySelector('input[type="password"]');
    if (!input) return;
    var realName = input.name;
    if (!realName) return;

    var hidden = document.createElement('input');
    hidden.type = 'hidden';
    hidden.name = realName;
    field.appendChild(hidden);

    input.removeAttribute('name');
    input.type = 'text';
    input.value = '';
    input.classList.add('pw-mask');
    input.setAttribute('autocomplete', 'new-password');
    input.setAttribute('autocapitalize', 'off');
    input.setAttribute('autocorrect', 'off');
    input.setAttribute('spellcheck', 'false');

    var real = '';
    var revealAt = -1;
    var timer = null;
    var DELAY = 1000;
    var DOT = '•';

    function paint() {
      var out = '';
      for (var i = 0; i < real.length; i++) {
        out += i === revealAt ? real[i] : DOT;
      }
      input.value = out;
      hidden.value = real;
    }

    function reveal(i) {
      revealAt = i;
      paint();
      if (timer) clearTimeout(timer);
      timer = setTimeout(function () {
        revealAt = -1;
        paint();
      }, DELAY);
    }

    function clearReveal() {
      revealAt = -1;
      if (timer) clearTimeout(timer);
    }

    input.addEventListener('beforeinput', function (e) {
      var s = input.selectionStart;
      var en = input.selectionEnd;
      var t = e.inputType || '';

      if (t === 'insertText' && e.data) {
        e.preventDefault();
        real = real.slice(0, s) + e.data + real.slice(en);
        var pos = s + e.data.length;
        paint();
        try { input.setSelectionRange(pos, pos); } catch (_) {}
        reveal(pos - 1);
      } else if (t === 'deleteContentBackward') {
        e.preventDefault();
        if (s !== en) {
          real = real.slice(0, s) + real.slice(en);
          paint();
          try { input.setSelectionRange(s, s); } catch (_) {}
        } else if (s > 0) {
          real = real.slice(0, s - 1) + real.slice(en);
          paint();
          try { input.setSelectionRange(s - 1, s - 1); } catch (_) {}
        }
        clearReveal();
      } else if (t === 'deleteContentForward') {
        e.preventDefault();
        if (s !== en) {
          real = real.slice(0, s) + real.slice(en);
          paint();
          try { input.setSelectionRange(s, s); } catch (_) {}
        } else if (s < real.length) {
          real = real.slice(0, s) + real.slice(s + 1);
          paint();
          try { input.setSelectionRange(s, s); } catch (_) {}
        }
        clearReveal();
      } else if (t === 'insertFromPaste' || t === 'insertFromDrop') {
        // handled by paste/drop listeners
      } else if (t.indexOf('delete') === 0) {
        e.preventDefault();
        if (s !== en) {
          real = real.slice(0, s) + real.slice(en);
          paint();
          try { input.setSelectionRange(s, s); } catch (_) {}
        } else {
          real = '';
          paint();
          try { input.setSelectionRange(0, 0); } catch (_) {}
        }
        clearReveal();
      } else {
        e.preventDefault();
      }
    });

    input.addEventListener('paste', function (e) {
      e.preventDefault();
      var text = (e.clipboardData || window.clipboardData).getData('text');
      if (!text) return;
      var s = input.selectionStart;
      var en = input.selectionEnd;
      real = real.slice(0, s) + text + real.slice(en);
      paint();
      try { input.setSelectionRange(s + text.length, s + text.length); } catch (_) {}
      clearReveal();
    });

    paint();
  }

  function init() {
    var fields = document.querySelectorAll('[data-pwfield]');
    for (var i = 0; i < fields.length; i++) enhance(fields[i]);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
