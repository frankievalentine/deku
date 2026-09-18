import { useEffect, useRef } from 'react';

interface SpinnerProps {
  /**
   * Fixed pixel size. Omit to scale with the viewport, which suits a
   * whole-panel loading state.
   */
  size?: number;
  className?: string;
}

/**
 * Loading indicator: three concentric rings, from Sam Herbert's SVG-Loaders
 * (MIT). Inlined rather than loaded as an image so it inherits the current
 * colour and follows light and dark themes.
 *
 * The source asset animates with SMIL, which CSS cannot pause, so the animation
 * is stopped through the SVG API when the visitor prefers reduced motion. The
 * centre ring then remains as a static indicator next to the status text.
 */
export default function Spinner({ size, className }: SpinnerProps) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;

    const query = window.matchMedia('(prefers-reduced-motion: reduce)');
    const apply = () => {
      if (query.matches) {
        svg.pauseAnimations();
      } else {
        svg.unpauseAnimations();
      }
    };

    apply();
    query.addEventListener('change', apply);
    return () => query.removeEventListener('change', apply);
  }, []);

  return (
    <svg
      ref={svgRef}
      className={['spinner-rings', className].filter(Boolean).join(' ')}
      style={size ? { width: size, height: size } : undefined}
      viewBox="0 0 45 45"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      focusable="false"
    >
      <g
        fill="none"
        fillRule="evenodd"
        transform="translate(1 1)"
        strokeWidth="2"
        stroke="currentColor"
      >
        <circle cx="22" cy="22" r="6" strokeOpacity="0">
          <animate
            attributeName="r"
            begin="1.5s"
            dur="3s"
            values="6;22"
            calcMode="linear"
            repeatCount="indefinite"
          />
          <animate
            attributeName="stroke-opacity"
            begin="1.5s"
            dur="3s"
            values="1;0"
            calcMode="linear"
            repeatCount="indefinite"
          />
          <animate
            attributeName="stroke-width"
            begin="1.5s"
            dur="3s"
            values="2;0"
            calcMode="linear"
            repeatCount="indefinite"
          />
        </circle>
        <circle cx="22" cy="22" r="6" strokeOpacity="0">
          <animate
            attributeName="r"
            begin="3s"
            dur="3s"
            values="6;22"
            calcMode="linear"
            repeatCount="indefinite"
          />
          <animate
            attributeName="stroke-opacity"
            begin="3s"
            dur="3s"
            values="1;0"
            calcMode="linear"
            repeatCount="indefinite"
          />
          <animate
            attributeName="stroke-width"
            begin="3s"
            dur="3s"
            values="2;0"
            calcMode="linear"
            repeatCount="indefinite"
          />
        </circle>
        <circle cx="22" cy="22" r="8">
          <animate
            attributeName="r"
            begin="0s"
            dur="1.5s"
            values="6;1;2;3;4;5;6"
            calcMode="linear"
            repeatCount="indefinite"
          />
        </circle>
      </g>
    </svg>
  );
}
