const ELLIPSIS = '...';

/**
 * Keep both ends of a string and remove characters from its middle until it fits.
 */
export function middleEllipsis(text: string, fits: (candidate: string) => boolean) {
  if (fits(text)) {
    return text;
  }

  const characters = Array.from(text);
  let best = fits(ELLIPSIS) ? ELLIPSIS : '';
  let low = 0;
  let high = characters.length;

  while (low <= high) {
    const kept = Math.floor((low + high) / 2);
    const headLength = Math.floor(kept / 2);
    const tailLength = kept - headLength;
    const head = characters.slice(0, headLength).join('').trimEnd();
    const tail = characters
      .slice(characters.length - tailLength)
      .join('')
      .trimStart();
    const candidate = `${head}${ELLIPSIS}${tail}`;
    if (fits(candidate)) {
      best = candidate;
      low = kept + 1;
    } else {
      high = kept - 1;
    }
  }
  return best;
}

/**
 * Render at most maxLines of text, using a middle ellipsis when it overflows.
 */
export function renderMiddleEllipsis(element: HTMLElement, text: string, maxLines: number) {
  const lineHeight = Number.parseFloat(getComputedStyle(element).lineHeight);
  const maxHeight = Math.ceil(lineHeight * maxLines);
  element.textContent = middleEllipsis(text, (candidate) => {
    element.textContent = candidate;
    return element.scrollHeight <= maxHeight;
  });
}
