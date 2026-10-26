export interface SegmentedControl {
  setValue(value: string | null): void;
  refresh(): void;
}

/**
 * Add a sliding selection indicator to a group of buttons with `data-value`.
 */
export function createSegmentedControl(
  root: HTMLElement,
  onChange: (value: string) => void,
): SegmentedControl {
  const indicator = root.querySelector<HTMLElement>('.segmented-control-indicator');
  const buttons = [...root.querySelectorAll<HTMLButtonElement>('[data-value]')];
  if (!indicator || buttons.length === 0) {
    throw new Error('segmented control needs an indicator and value buttons');
  }

  let value: string | null = null;

  const position = () => {
    const selected = buttons.find((button) => button.dataset.value === value);
    if (!selected || root.offsetWidth === 0) {
      indicator.style.opacity = '0';
      return;
    }
    indicator.style.width = `${selected.offsetWidth}px`;
    indicator.style.transform = `translateX(${selected.offsetLeft}px)`;
    indicator.style.opacity = '1';
  };

  buttons.forEach((button) => {
    button.addEventListener('click', () => {
      const next = button.dataset.value;
      if (next !== undefined) {
        onChange(next);
      }
    });
  });

  const observer = new ResizeObserver(position);
  observer.observe(root);
  buttons.forEach((button) => observer.observe(button));
  requestAnimationFrame(() => {
    root.dataset.ready = 'true';
    position();
  });

  return {
    setValue(next) {
      value =
        next === null || buttons.some((button) => button.dataset.value === next) ? next : null;
      buttons.forEach((button) => {
        button.setAttribute('aria-pressed', String(button.dataset.value === value));
      });
      position();
    },
    refresh: position,
  };
}
