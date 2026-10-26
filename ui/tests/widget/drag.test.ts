import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

import { Window, type PointerEvent } from 'happy-dom';
import { describe, expect, it, vi } from 'vite-plus/test';

// Exercise the injected script against a DOM, without granting synthetic browser events trust.
function install() {
  const window = new Window({ url: 'http://aidget.localhost/content/note/index.html?v=2-token' });
  window.document.body.innerHTML =
    '<p id="text">Note</p><button><span id="button-child">OK</span></button><div data-aidget-no-drag><span id="selectable">Copy</span></div>';
  const posted: unknown[] = [];
  const parent = { postMessage: (data: unknown) => posted.push(data) };
  const listeners = new Map<string, (event: unknown) => void>();
  const document = window.document;
  const add = document.addEventListener.bind(document);
  document.addEventListener = ((type: string, listener: (event: unknown) => void) => {
    listeners.set(type, listener);
    add(type, listener);
  }) as typeof document.addEventListener;
  const windowListeners = new Map<string, (event: unknown) => void>();
  const sandboxWindow = {
    parent,
    addEventListener: (type: string, listener: (event: unknown) => void) =>
      windowListeners.set(type, listener),
    getComputedStyle: window.getComputedStyle.bind(window),
    innerWidth: 1024,
    innerHeight: 768,
  };
  runInNewContext(readFileSync(new URL('../../src/widget/drag.js', import.meta.url), 'utf8'), {
    window: sandboxWindow,
    document,
    location: window.location,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    URLSearchParams,
    setTimeout,
    clearTimeout,
  });
  const send = (data: unknown) => windowListeners.get('message')?.({ source: parent, data });
  send({
    source: 'aidget-host',
    type: 'drag-config',
    revision: 2,
    token: 'token',
    dragMode: 'all',
  });
  const pointer = (type: string, fields: Partial<PointerEvent> = {}) => {
    const preventDefault = vi.fn();
    listeners.get(type)?.({
      isTrusted: true,
      pointerType: 'mouse',
      isPrimary: true,
      pointerId: 1,
      button: 0,
      buttons: 1,
      clientX: 20,
      clientY: 20,
      target: document.querySelector('#text'),
      preventDefault,
      stopImmediatePropagation: vi.fn(),
      ...fields,
    });
    return preventDefault;
  };
  return { window, document, posted, pointer, send, windowListeners };
}

describe('injected widget dragging', () => {
  it('switches the cursor to grabbing as soon as a drag starts', () => {
    const h = install();
    h.pointer('pointerdown');
    expect(h.document.documentElement.hasAttribute('data-aidget-dragging')).toBe(true);
    h.pointer('pointerup');
    expect(h.document.documentElement.hasAttribute('data-aidget-dragging')).toBe(false);
  });

  it('starts once at four pixels and leaves clicks below the threshold alone', () => {
    const h = install();
    h.pointer('pointerdown');
    h.pointer('pointermove', { clientX: 23 });
    expect(h.posted).toEqual([]);
    h.pointer('pointermove', { clientX: 24 });
    h.pointer('pointermove', { clientX: 30 });
    expect(h.posted).toHaveLength(1);
    expect(h.posted[0]).toMatchObject({
      type: 'drag',
      phase: 'start',
      revision: 2,
      token: 'token',
    });
  });

  it.each([
    'button span',
    '[data-aidget-no-drag] span',
    'input',
    '[contenteditable] span',
    '[role="slider"] span',
    'a span',
  ])('preserves %s and its descendants', (selector) => {
    const h = install();
    h.document.body.insertAdjacentHTML(
      'beforeend',
      '<input><div contenteditable><span>edit</span></div><div role="slider"><span>slide</span></div><a href="#"><span>link</span></a>',
    );
    const target = h.document.querySelector(selector)!;
    expect(h.pointer('pointerdown', { target }).mock.calls).toEqual([]);
    h.pointer('pointermove', { clientX: 50 });
    expect(h.posted).toEqual([]);
  });

  it('ignores right buttons, touch, untrusted events and released pointers', () => {
    for (const fields of [{ button: 2 }, { pointerType: 'touch' }, { isTrusted: false }]) {
      const h = install();
      h.pointer('pointerdown', fields);
      h.pointer('pointermove', { clientX: 50 });
      expect(h.posted).toEqual([]);
    }
    const h = install();
    h.pointer('pointerdown');
    h.pointer('pointermove', { clientX: 50, buttons: 0 });
    expect(h.posted).toEqual([]);
  });

  it.each(['pointerup', 'pointercancel'])('clears a candidate on %s', (type) => {
    const h = install();
    h.pointer('pointerdown');
    h.pointer(type);
    h.pointer('pointermove', { clientX: 50 });
    expect(h.posted).toEqual([]);
  });

  it('clears on blur and native drag completion, suppressing only the drag click', () => {
    const h = install();
    h.pointer('pointerdown');
    h.windowListeners.get('blur')?.({});
    h.pointer('pointermove', { clientX: 50 });
    expect(h.posted).toEqual([]);
    h.pointer('pointerdown');
    h.pointer('pointermove', { clientX: 24 });
    h.send({ source: 'aidget-host', type: 'drag-end', revision: 2, token: 'token', gesture: 2 });
    h.pointer('pointermove', { clientX: 50 });
    expect(h.posted).toHaveLength(1);
    expect(h.pointer('click')).toHaveBeenCalledOnce();
    expect(h.pointer('click')).not.toHaveBeenCalled();
  });

  it('ignores stale config and handle mode', () => {
    const h = install();
    h.send({
      source: 'aidget-host',
      type: 'drag-config',
      revision: 1,
      token: 'token',
      dragMode: 'all',
    });
    h.send({
      source: 'aidget-host',
      type: 'drag-config',
      revision: 2,
      token: 'token',
      dragMode: 'handle',
    });
    h.pointer('pointerdown');
    h.pointer('pointermove', { clientX: 50 });
    expect(h.posted).toEqual([]);
  });

  it('does not consume a new button click after a native drag', () => {
    const h = install();
    h.pointer('pointerdown');
    h.pointer('pointermove', { clientX: 24 });
    h.send({ source: 'aidget-host', type: 'drag-end', revision: 2, token: 'token', gesture: 1 });
    h.pointer('pointerdown', { target: h.document.querySelector('button')! });
    expect(h.pointer('click')).not.toHaveBeenCalled();
  });

  it('excludes scrollbar tracks while allowing inline text', () => {
    const h = install();
    const target = h.document.createElement('div');
    target.style.overflowY = 'scroll';
    h.document.body.appendChild(target);
    for (const [key, value] of Object.entries({
      offsetWidth: 100,
      offsetHeight: 100,
      clientWidth: 80,
      clientHeight: 100,
      scrollHeight: 200,
    })) {
      Object.defineProperty(target, key, { value });
    }
    target.getBoundingClientRect = () => new h.window.DOMRect(0, 0, 100, 100);
    h.pointer('pointerdown', { target, clientX: 90 });
    h.pointer('pointermove', { clientX: 110 });
    expect(h.posted).toEqual([]);
    const text = h.document.querySelector('#text')!;
    text.innerHTML = '<span>Inline text</span>';
    const span = text.firstElementChild!;
    Object.defineProperty(span, 'offsetWidth', { value: 50 });
    h.pointer('pointerdown', { target: span });
    h.pointer('pointermove', { clientX: 24 });
    expect(h.posted).toHaveLength(1);
  });

  it('preserves the viewport scrollbar with the default visible overflow', () => {
    const h = install();
    const target = h.document.documentElement;
    for (const [key, value] of Object.entries({
      offsetWidth: 100,
      offsetHeight: 100,
      clientWidth: 80,
      clientHeight: 100,
      scrollHeight: 200,
    })) {
      Object.defineProperty(target, key, { value });
    }
    target.getBoundingClientRect = () => new h.window.DOMRect(0, 0, 100, 100);
    h.pointer('pointerdown', { target, clientX: 90 });
    h.pointer('pointermove', { clientX: 110 });
    expect(h.posted).toEqual([]);
  });

  it('uses viewport coordinates for a document that has already scrolled', () => {
    const h = install();
    const target = h.document.documentElement;
    for (const [key, value] of Object.entries({
      offsetWidth: 100,
      offsetHeight: 100,
      clientWidth: 80,
      clientHeight: 100,
      scrollHeight: 300,
      scrollWidth: 200,
    })) {
      Object.defineProperty(target, key, { value });
    }
    target.getBoundingClientRect = () => new h.window.DOMRect(0, -120, 100, 100);
    h.pointer('pointerdown', { target });
    h.pointer('pointermove', { clientX: 24 });
    expect(h.posted).toHaveLength(1);
  });
});
