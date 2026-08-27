/** @type {import('prettier').Config} */
export default {
  singleQuote: true,
  printWidth: 120,
  htmlWhitespaceSensitivity: 'ignore',
  plugins: ['prettier-plugin-svelte', 'prettier-plugin-tailwindcss'],
  overrides: [{ files: '*.svelte', options: { parser: 'svelte' } }],
};
