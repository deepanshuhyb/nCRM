/** Fine frosted grain for the reeded hero glass. Decorative only. */
export function Grain() {
  return (
    <svg className="grain" aria-hidden="true" focusable="false">
      <filter id="ncrm-grain">
        <feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves={2} stitchTiles="stitch" />
        <feColorMatrix type="saturate" values="0" />
      </filter>
      <rect width="100%" height="100%" filter="url(#ncrm-grain)" />
    </svg>
  );
}
