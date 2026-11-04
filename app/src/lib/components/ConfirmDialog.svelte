<script lang="ts">
	import { fly, fade } from 'svelte/transition';
	import { confirmStore } from '$lib/stores/confirm';

	let dialogEl = $state<HTMLDivElement | null>(null);

	function cancel() {
		$confirmStore.resolve?.(false);
	}

	function confirmAction() {
		$confirmStore.resolve?.(true);
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') cancel();
		if (e.key === 'Enter') confirmAction();
	}

	$effect(() => {
		if ($confirmStore.open) dialogEl?.focus();
	});
</script>

{#if $confirmStore.open}
	<div class="fixed inset-0 z-[80] bg-[var(--color-scrim)]" transition:fade={{ duration: 150 }}>
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="absolute inset-0" onclick={cancel}></div>

		<div class="relative flex min-h-full items-center justify-center px-4">
			<div
				bind:this={dialogEl}
				role="alertdialog"
				aria-modal="true"
				aria-label={$confirmStore.title}
				tabindex="-1"
				onkeydown={onKeydown}
				transition:fly={{ y: 12, duration: 180 }}
				class="relative w-full max-w-sm border border-[var(--clr-border-light)] border-t-2 border-t-[var(--clr-rule-firm)] bg-[var(--bg-card)] shadow-[var(--shadow-float)] overflow-hidden outline-none"
			>
				<div class="px-6 pt-6 pb-5">
					<h2 class="text-base font-semibold text-[var(--text-primary)] tracking-tight head-wrap">{$confirmStore.title}</h2>
					<p class="mt-2 font-serif text-[0.9375rem] text-[var(--text-muted)] whitespace-pre-line">{$confirmStore.message}</p>

					<div class="mt-6 flex justify-end gap-2">
						<button
							type="button"
							onclick={cancel}
							class="btn btn-secondary"
						>
							{$confirmStore.cancelLabel}
						</button>
						<button
							type="button"
							onclick={confirmAction}
							class="btn {$confirmStore.danger ? 'border border-[var(--color-danger)] bg-[var(--color-danger)] text-[var(--color-status-ink)] hover:opacity-90 active:translate-y-px' : 'btn-primary'}"
						>
							{$confirmStore.confirmLabel}
						</button>
					</div>
				</div>
			</div>
		</div>
	</div>
{/if}
