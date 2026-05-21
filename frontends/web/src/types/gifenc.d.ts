declare module "gifenc" {
  export type GifPalette = number[][];

  export type GifEncoder = {
    bytes: () => Uint8Array;
    finish: () => void;
    writeFrame: (
      index: Uint8Array,
      width: number,
      height: number,
      options?: {
        delay?: number;
        first?: boolean;
        palette?: GifPalette;
        repeat?: number;
      },
    ) => void;
  };

  export function GIFEncoder(options?: {
    auto?: boolean;
    initialCapacity?: number;
  }): GifEncoder;

  export function applyPalette(
    rgba: Uint8Array | Uint8ClampedArray,
    palette: GifPalette,
    format?: "rgb444" | "rgb565" | "rgba4444",
  ): Uint8Array;

  export function quantize(
    rgba: Uint8Array | Uint8ClampedArray,
    maxColors: number,
    options?: {
      clearAlpha?: boolean;
      clearAlphaColor?: number;
      clearAlphaThreshold?: number;
      format?: "rgb444" | "rgb565" | "rgba4444";
      oneBitAlpha?: boolean | number;
    },
  ): GifPalette;
}
