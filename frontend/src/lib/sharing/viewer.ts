/** Desktop origins are not public viewer URLs. Leave the field empty unless configured. */
export function publicViewerUrl(runtime: 'browser' | 'desktop'): string | null {
  const configured = import.meta.env.VITE_PUBLIC_APP_URL as string | undefined;
  return configured || (runtime === 'browser' ? new URL('./', document.baseURI).href : null);
}
