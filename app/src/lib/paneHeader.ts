/**
 * What sits at the top and the foot of the Tasks pane: the app shell's account
 * menu, and its sync status and version. The shell supplies them as snippets,
 * so lib stays free of the shell's code.
 */
import { getContext, setContext, type Snippet } from 'svelte';

const HEADER = Symbol('tasks-pane-header');
const FOOTER = Symbol('tasks-pane-footer');

export const setPaneHeader = (header: Snippet) => setContext(HEADER, header);
export const getPaneHeader = () => getContext<Snippet | undefined>(HEADER);
export const setPaneFooter = (footer: Snippet) => setContext(FOOTER, footer);
export const getPaneFooter = () => getContext<Snippet | undefined>(FOOTER);
