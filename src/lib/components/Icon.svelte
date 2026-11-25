<script lang="ts" module>
  // Line icons drawn on a 24 x 24 grid. Add new ones here; emoji are never used as icons.
  const PATHS = {
    check: "M5 12.5l4.5 4.5L19 7.5",
    "check-circle": "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18M8 12.2l2.8 2.8L16.2 9.6",
    "chevron-down": "M6 9l6 6l6-6",
    "chevron-up": "M6 15l6-6l6 6",
    close: "M6 6l12 12M18 6L6 18",
    error: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18M12 7.5v5.5M12 16.5v.01",
    info: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18M12 11v5.5M12 7.5v.01",
    minus: "M5 12h14",
    plus: "M12 5v14M5 12h14",
    more:"M4 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0M11 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0M18 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0",
    trash: "M4 7h16M9 7V4h6v3M6.5 7l1 13h9l1-13M10 11v5M14 11v5",
    upload: "M12 16V4M7 9l5-5l5 5M4 16v4h16v-4",
    warning: "M12 4L2.5 20h19L12 4zM12 10v4.5M12 17.3v.01",
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 16 }: { name: IconName; size?: number } = $props();
</script>

<!-- Decorative: the text next to it, or the button's label, carries the meaning. -->
<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
  data-icon={name}
>
  <path d={PATHS[name]} />
</svg>

<style>
  svg {
    flex: none;
  }
</style>
