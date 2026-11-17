// Mirrors CONTRACT §3 and §5 one-to-one.

export type AxisId = 'x' | 'y' | 'rz' | 'slider';
export const AXES: readonly AxisId[] = ['x', 'y', 'rz', 'slider'];
export const AXIS_LABELS: Record<AxisId, string> = {
  x: 'X',
  y: 'Y',
  rz: 'Rz (Twist)',
  slider: 'Slider (Throttle)',
};

export interface Calibration {
  min: number;
  center: number;
  max: number;
}

export interface Deadzone {
  center: number;
  min: number;
  max: number;
}

/** Output magnitude for deflection magnitude, mirrored for negative deflection. Point x is a multiple of 1/16. */
export type Curve = { type: 'linear' } | { type: 'points'; points: [number, number][] };

export interface AxisConfig {
  calibration: Calibration;
  invert: boolean;
  deadzone: Deadzone;
  curve: Curve;
  sensitivity: number;
  target: AxisId | null;
}

export type HatMode = 'hat' | 'buttons' | 'both';

export interface ButtonBinding {
  output: number | null;
}

export interface KeyBinding {
  button: number;
  keys: string[];
}

export interface DeviceConfig {
  axes: Record<AxisId, AxisConfig>;
  hatMode: HatMode;
  buttons: ButtonBinding[];
  shiftButton: number | null;
  buttonsShifted: ButtonBinding[];
  keyBindings: KeyBinding[];
}

export interface Profile {
  id: string;
  name: string;
  exePaths: string[];
  isDefault: boolean;
  config: DeviceConfig;
}

export interface Settings {
  autostart: boolean;
  switchingPaused: boolean;
  matchMode: 'foreground' | 'running';
}

export interface AppState {
  deviceConnected: boolean;
  driverInstalled: boolean;
  driverVersion: string | null;
  activeProfileId: string | null;
  matchedExe: string | null;
  settings: Settings;
}

export interface WindowInfo {
  pid: number;
  exePath: string;
  title: string;
}

export interface InputFrame {
  raw: { x: number; y: number; rz: number; slider: number; hat: number; buttons: number };
  /** Axes are -OUT_MAX..OUT_MAX. */
  processed: { x: number; y: number; rz: number; slider: number; hat: number; buttons: number };
  seq: number;
}

export interface ProfileSwitched {
  profileId: string;
  exePath: string | null;
  reason: 'foreground' | 'running' | 'manual' | 'default';
}

export const PHYSICAL_BUTTONS = 12;
export const OUTPUT_BUTTONS = 32;

function defaultAxis(id: AxisId): AxisConfig {
  return {
    calibration: { min: 0, center: 32768, max: 65535 },
    invert: false,
    deadzone: { center: 0, min: 0, max: 0 },
    curve: { type: 'linear' },
    sensitivity: 1,
    target: id,
  };
}

export function defaultDeviceConfig(): DeviceConfig {
  return {
    axes: { x: defaultAxis('x'), y: defaultAxis('y'), rz: defaultAxis('rz'), slider: defaultAxis('slider') },
    hatMode: 'hat',
    buttons: Array.from({ length: PHYSICAL_BUTTONS }, (_, i) => ({ output: i + 1 })),
    shiftButton: null,
    buttonsShifted: Array.from({ length: PHYSICAL_BUTTONS }, () => ({ output: null })),
    keyBindings: [],
  };
}
