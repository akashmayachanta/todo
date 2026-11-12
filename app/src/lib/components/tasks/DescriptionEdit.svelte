<script lang="ts">
	import { tick } from 'svelte';
	import { plainText } from '$lib/links';
	import LinkedText from './LinkedText.svelte';

	/**
	 * A project's or category's description, edited in place. Shown as typed, links
	 * included (LinkedText); clicking it, away from a link, opens a textarea. Enter
	 * adds a line, Ctrl+Enter or leaving the box saves, Escape drops the edit.
	 * An `<a>` can't sit in a `<button>`, so the text is a clickable div with a
	 * screen-reader button beside it for the keyboard.
	 */
	let {
		value,
		label,
		placeholder = 'Add a description',
		showEmpty = true,
		singleLine = false,
		editing = $bindable(false),
		onsave,
	}: {
		value: string | null;
		/** What it describes, for the aria labels: "Cleaning". */
		label: string;
		placeholder?: string;
		/** With no description, show the placeholder to click (else nothing until `editing`). */
		showEmpty?: boolean;
		/** Shown on one line, its line breaks as spaces and cut with "…"; editing shows it all. */
		singleLine?: boolean;
		editing?: boolean;
		/** The new text, or null to clear it. Only called when it changed. */
		onsave: (text: string | null) => void;
	} = $props();

	let draft = $state('');
	let area = $state<HTMLTextAreaElement | null>(null);
	let wasEditing = false;

	// Opening (from a click here or from outside, e.g. a menu) starts from the saved text.
	$effect(() => {
		if (editing && !wasEditing) {
			draft = value ?? '';
			tick().then(() => {
				area?.focus();
				area?.setSelectionRange(draft.length, draft.length);
				fit();
			});
		}
		wasEditing = editing;
	});

	function fit() {
		if (!area) return;
		area.style.height = 'auto';
		area.style.height = `${area.scrollHeight}px`;
	}

	function save() {
		if (!editing) return;
		editing = false;
		const text = draft.trim();
		if (text !== (value ?? '').trim()) onsave(text || null);
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			editing = false;
		} else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
			e.preventDefault();
			save();
		}
	}

	function onTextClick(e: MouseEvent) {
		if ((e.target as Element).closest('a')) return;
		editing = true;
	}
</script>

{#if editing}
	<textarea
		bind:this={area}
		bind:value={draft}
		class="desc-edit-input"
		rows="1"
		{placeholder}
		aria-label="Description of {label}"
		oninput={fit}
		onkeydown={onKeydown}
		onblur={save}
	></textarea>
{:else if value}
	<div class="desc-edit">
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="desc-edit-text" class:desc-edit-one={singleLine} title={singleLine ? plainText(value) : undefined} onclick={onTextClick}>
			<LinkedText text={singleLine ? value.replace(/\s*\n\s*/g, ' ') : value} />
		</div>
		<button type="button" class="sr-only focus:not-sr-only desc-edit-open" onclick={() => (editing = true)}>Edit description of {label}</button>
	</div>
{:else if showEmpty}
	<button type="button" class="desc-edit-empty" onclick={() => (editing = true)}>{placeholder}</button>
{/if}

<style>
	.desc-edit-text,
	.desc-edit-empty,
	.desc-edit-input {
		display: block;
		width: 100%;
		margin: 0;
		padding: 0.125rem 0;
		font-family: var(--font-prose);
		font-size: 0.875rem;
		line-height: 1.5;
		text-align: left;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.desc-edit-text {
		color: var(--text-secondary);
		cursor: text;
	}

	/* One quiet line under a heading, as in Todoist's sections */
	.desc-edit-one {
		overflow: hidden;
		padding: 0;
		font-size: 0.75rem;
		color: var(--text-muted);
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	.desc-edit-empty {
		color: var(--text-faint);
		cursor: text;
	}

	.desc-edit-empty:hover {
		color: var(--text-muted);
	}

	.desc-edit-input {
		resize: none;
		overflow: hidden;
		field-sizing: content;
		border: 0;
		border-radius: var(--radius-control);
		background: transparent;
		color: var(--text-primary);
		outline: none;
		box-shadow: none;
	}

	.desc-edit-input:focus {
		outline: none;
		box-shadow: none;
	}

	.desc-edit-open {
		font-size: 0.75rem;
		color: var(--text-muted);
	}
</style>
