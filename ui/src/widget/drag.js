// Host-owned drag listener for sandboxed widget documents.
(function () {
  if (!/^\/content\/[^/]+\/index\.html$/.test(location.pathname)) {
    return;
  }
  if (location.hostname !== 'localhost' && location.hostname !== 'aidget.localhost') {
    return;
  }
  var version = new URLSearchParams(location.search).get('v') || '';
  var separator = version.indexOf('-');
  var revision = Number(version.slice(0, separator));
  var token = version.slice(separator + 1);
  if (!Number.isSafeInteger(revision) || revision <= 0 || !token) {
    return;
  }

  var excluded = [
    '[data-aidget-no-drag]',
    'a[href]',
    'button',
    'input',
    'select',
    'textarea',
    'label',
    'summary',
    'iframe',
    'object',
    'embed',
    'audio[controls]',
    'video[controls]',
    '[contenteditable]:not([contenteditable="false"])',
    '[draggable="true"]',
  ];
  [
    'button',
    'link',
    'checkbox',
    'radio',
    'switch',
    'slider',
    'spinbutton',
    'textbox',
    'searchbox',
    'combobox',
    'listbox',
    'option',
    'menuitem',
    'menuitemcheckbox',
    'menuitemradio',
    'tab',
    'treeitem',
    'scrollbar',
  ].forEach(function (role) {
    excluded.push('[role~="' + role + '"]');
  });
  var selector = excluded.join(',');
  var enabled = false;
  var gesture = null;
  var sequence = 0;
  var suppressClick = false;
  var clickTimer;

  function signal(phase, id) {
    window.parent.postMessage(
      {
        source: 'aidget-widget',
        type: 'drag',
        phase: phase,
        revision: revision,
        token: token,
        gesture: id,
      },
      '*',
    );
  }

  function reset(notify) {
    if (notify && gesture && gesture.started) {
      signal('cancel', gesture.id);
    }
    if (gesture && gesture.started) {
      // Native dragging may consume the release and click; only suppress its trailing click.
      clearTimeout(clickTimer);
      clickTimer = setTimeout(function () {
        suppressClick = false;
      }, 1000);
    }
    gesture = null;
    document.documentElement.removeAttribute('data-aidget-dragging');
  }

  function exempt(event) {
    var path = event.composedPath ? event.composedPath() : [event.target];
    return path.some(function (node) {
      if (!(node instanceof Element)) {
        return false;
      }
      if (node.closest(selector)) {
        return true;
      }
      // Scrollbars belong to their scroll container, not a distinct DOM target.
      if (!(node instanceof HTMLElement) || node.offsetWidth === 0) {
        return false;
      }
      var style = window.getComputedStyle(node);
      var viewport = node === document.scrollingElement || node === document.documentElement;
      var vertical =
        node.scrollHeight > node.clientHeight &&
        (/^(auto|scroll)$/.test(style.overflowY) ||
          (viewport && !/^(hidden|clip)$/.test(style.overflowY)));
      var horizontal =
        node.scrollWidth > node.clientWidth &&
        (/^(auto|scroll)$/.test(style.overflowX) ||
          (viewport && !/^(hidden|clip)$/.test(style.overflowX)));
      if (!vertical && !horizontal) {
        return false;
      }
      var rect = node.getBoundingClientRect();
      var x = viewport
        ? event.clientX
        : ((event.clientX - rect.left) * node.offsetWidth) / rect.width;
      var y = viewport
        ? event.clientY
        : ((event.clientY - rect.top) * node.offsetHeight) / rect.height;
      return (
        (vertical && (x >= node.clientLeft + node.clientWidth || x < node.clientLeft)) ||
        (horizontal && (y >= node.clientTop + node.clientHeight || y < node.clientTop))
      );
    });
  }

  window.addEventListener('message', function (event) {
    if (event.source !== window.parent) {
      return;
    }
    var data = event.data;
    if (
      !data ||
      data.source !== 'aidget-host' ||
      data.revision !== revision ||
      data.token !== token
    ) {
      return;
    }
    if (data.type === 'drag-config' && (data.dragMode === 'all' || data.dragMode === 'handle')) {
      reset(true);
      enabled = data.dragMode === 'all';
      document.documentElement.setAttribute('data-aidget-drag-mode', data.dragMode);
    } else if (data.type === 'drag-end' && gesture && data.gesture === gesture.id) {
      reset(false);
    }
  });

  document.addEventListener(
    'pointerdown',
    function (event) {
      if (
        !event.isTrusted ||
        !enabled ||
        gesture ||
        event.pointerType !== 'mouse' ||
        !event.isPrimary ||
        event.button !== 0
      ) {
        return;
      }
      suppressClick = false;
      clearTimeout(clickTimer);
      if (exempt(event)) {
        return;
      }
      gesture = {
        id: ++sequence,
        pointer: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        started: false,
      };
      // Prevent browser text/image dragging only on surfaces reserved for moving the window.
      document.documentElement.setAttribute('data-aidget-dragging', '');
      event.preventDefault();
    },
    true,
  );

  document.addEventListener(
    'pointermove',
    function (event) {
      if (!gesture || event.pointerId !== gesture.pointer) {
        return;
      }
      if (!(event.buttons & 1)) {
        reset(true);
        return;
      }
      if (gesture.started || Math.hypot(event.clientX - gesture.x, event.clientY - gesture.y) < 4) {
        return;
      }
      gesture.started = true;
      suppressClick = true;
      signal('start', gesture.id);
    },
    true,
  );

  ['pointerup', 'pointercancel'].forEach(function (type) {
    document.addEventListener(
      type,
      function (event) {
        if (gesture && event.pointerId === gesture.pointer) {
          reset(true);
        }
      },
      true,
    );
  });
  ['blur', 'pagehide'].forEach(function (type) {
    window.addEventListener(type, function () {
      reset(true);
    });
  });
  document.addEventListener(
    'click',
    function (event) {
      if (!suppressClick || !event.isTrusted) {
        return;
      }
      suppressClick = false;
      clearTimeout(clickTimer);
      event.preventDefault();
      event.stopImmediatePropagation();
    },
    true,
  );
  document.addEventListener(
    'dragstart',
    function (event) {
      if (enabled && !exempt(event)) {
        event.preventDefault();
      }
    },
    true,
  );
  document.addEventListener(
    'DOMContentLoaded',
    function () {
      var style = document.createElement('style');
      style.textContent =
        'html[data-aidget-drag-mode="all"] { cursor: grab; }' +
        'html[data-aidget-drag-mode="all"] :is(' +
        selector +
        ') { cursor: auto; }' +
        'html[data-aidget-dragging], html[data-aidget-dragging] * { cursor: grabbing !important; }';
      document.head.appendChild(style);
    },
    { once: true },
  );
})();
