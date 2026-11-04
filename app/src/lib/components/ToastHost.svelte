<script lang="ts">
	import { fly } from 'svelte/transition';
	import { toast, type ToastVariant } from '$lib/stores/toast';

	// The single place toast icons are defined — every screen's toasts route through here.
	const ICON_PATHS: Record<ToastVariant, string> = {
		success: 'M5 13l4 4L19 7',
		error: 'M6 18L18 6M6 6l12 12',
		info: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
	};

	const VARIANT_CLASS: Record<ToastVariant, string> = {
		success: 'bg-[var(--color-success)] border-[var(--color-success)] text-[var(--color-status-ink)]',
		error: 'bg-[var(--color-danger)] border-[var(--color-danger)] text-[var(--color-status-ink)]',
		info: 'bg-[var(--bg-card)] border-[var(--clr-border-light)] border-t-2 border-t-[var(--clr-rule-firm)] text-[var(--text-primary)]',
	};
</script>

<div class="fixed top-4 left-1/2 -translate-x-1/2 z-[70] flex flex-col items-center gap-2 pointer-events-none w-max max-w-[calc(100%-2rem)]">
	{#each $toast as item (item.id)}
		<div
			transition:fly={{ y: -12, duration: 200 }}
			class="pointer-events-auto flex items-center gap-2 max-w-sm px-4 py-3 rounded-sm border shadow-[var(--shadow-float)] text-sm font-medium {VARIANT_CLASS[item.variant]}"
		>
			<svg aria-hidden="true" class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d={ICON_PATHS[item.variant]} />
			</svg>
			<span class="min-w-0">{item.message}</span>
			<button
				type="button"
				onclick={() => toast.dismiss(item.id)}
				aria-label="Dismiss notification"
				class="ml-1 shrink-0 p-0.5 rounded opacity-70 hover:opacity-100 transition-opacity"
			>
				<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M6 18L18 6M6 6l12 12" />
				</svg>
			</button>
		</div>
	{/each}
</div>
