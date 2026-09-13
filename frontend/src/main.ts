import './style.css';
import App from './App.svelte';
import { mount } from 'svelte';
import { prepareOfflineDocument } from './lib/offline/bootstrap';

const target = document.getElementById('app')!;
void prepareOfflineDocument(target).then((ready) => {
  if (!ready) return;
  target.replaceChildren();
  mount(App, { target });
});
