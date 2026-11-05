<script lang="ts">
	import { displayNames, nameFor, photos } from './profile';

	/** The profile photo, or the display name's first letter on the brand colour. `name` is the username. */
	let { name, size = 24 }: { name: string | null; size?: number } = $props();

	let src = $derived(name ? $photos[name] : undefined);
	let initial = $derived(name ? nameFor($displayNames, name).trim().charAt(0).toUpperCase() || '?' : '?');
</script>

<span class="avatar" style="--avatar-size: {size}px" aria-hidden="true">
	{#if src}<img {src} alt="" />{:else}{initial}{/if}
</span>

<style>
	.avatar {
		display: inline-grid;
		place-items: center;
		flex-shrink: 0;
		width: var(--avatar-size);
		height: var(--avatar-size);
		border-radius: 999px;
		overflow: hidden;
		background: var(--color-brand);
		color: var(--color-brand-text);
		font-family: var(--font-display);
		font-weight: 600;
		font-size: calc(var(--avatar-size) * 0.44);
		line-height: 1;
		user-select: none;
	}
	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
</style>
