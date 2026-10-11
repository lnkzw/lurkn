<!--
  A six-toothed gear. Fewer, deeper teeth than a realistic gear on purpose: at
  15px an eight-tooth outline blurs into a hexagon, which reads as a nut rather
  than a settings control.
-->
<script lang="ts">
  interface Props {
    size?: number;
  }

  let { size = 16 }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  aria-hidden="true"
  fill="currentColor"
  stroke="none"
>
  <path fill-rule="evenodd" d="M10.34 4.79L10.15 0.75L13.85 0.75L13.66 4.79A7.40 7.40 0 0 1 17.41 6.95L17.41 6.95L20.81 4.77L22.67 7.98L19.08 9.84A7.40 7.40 0 0 1 19.08 14.16L19.08 14.16L22.67 16.02L20.81 19.23L17.41 17.05A7.40 7.40 0 0 1 13.66 19.21L13.66 19.21L13.85 23.25L10.15 23.25L10.34 19.21A7.40 7.40 0 0 1 6.59 17.05L6.59 17.05L3.19 19.23L1.33 16.02L4.92 14.16A7.40 7.40 0 0 1 4.92 9.84L4.92 9.84L1.33 7.98L3.19 4.77L6.59 6.95A7.40 7.40 0 0 1 10.34 4.79ZM15.90 12.00A3.90 3.90 0 1 0 8.10 12.00A3.90 3.90 0 1 0 15.90 12.00Z" />
</svg>
