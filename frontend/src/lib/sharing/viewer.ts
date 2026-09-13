/** Desktop shares open in the public GitHub Pages viewer. */
export function publicViewerUrl(runtime: 'browser' | 'desktop'): string {
  const configured = import.meta.env.VITE_PUBLIC_APP_URL as string | undefined;
  return (
    configured ||
    (runtime === 'browser'
      ? new URL('./', document.baseURI).href
      : 'https://valtrius.github.io/satisfactory-flow-synthetizer/')
  );
}
