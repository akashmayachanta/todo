<script lang="ts">
	/** Today's icon: a calendar tile showing the day of the month. */
	let { date, class: className = '', strokeWidth = 1.75 }: {
		/** Today, `YYYY-MM-DD`. */
		date: string;
		class?: string;
		strokeWidth?: number;
	} = $props();

	let day = $derived(Number(date.slice(8)));
</script>

<svg aria-hidden="true" class={className} fill="none" stroke="currentColor" viewBox="0 0 24 24">
	<path stroke-linecap="round" stroke-linejoin="round" stroke-width={strokeWidth} d="M8 6V3m8 3V3M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
	<text x="12" y="17.5" text-anchor="middle" fill="currentColor" stroke="none" font-size="11" font-weight="700">{day}</text>
</svg>
