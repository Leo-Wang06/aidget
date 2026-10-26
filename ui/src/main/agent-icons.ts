const files = import.meta.glob('../../assets/agent-icons/*.{svg,png}', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>;

/** A bundled Agent logo, rendered as an image when colored and a mask when monochrome. */
export interface AgentIconAsset {
  /** Bundled asset URL. */
  url: string;
  /** Whether to render the SVG as a mask in the current text color. */
  monochrome: boolean;
}

const assets = new Map<string, AgentIconAsset>(
  Object.entries(files).map(([path, url]) => {
    const filename = path.split('/').pop()!;
    const name = filename.replace(/\.(svg|png)$/, '');
    return [
      name,
      {
        url,
        monochrome: filename.endsWith('.svg') && !name.endsWith('-color'),
      },
    ];
  }),
);

/** Resolve one bundled vendor icon, or undefined when the registry uses none. */
export function agentIcon(name: string) {
  return assets.get(name);
}
