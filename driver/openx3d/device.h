/* Per-device state. Included via driver.h. */
#pragma once

#define X3D_DRIVER_VERSION_BCD 0x0010 /* 0.1.0 */
#define X3D_FLAG_FROM_REGISTRY 0x01   /* active config is the one stored in the registry */
#define X3D_FLAG_IDENTITY      0x02   /* active config == pipeline_identity() */

#define X3D_VENDOR_REPORT_SIZE 11     /* ID, raw[7], seq u16, flags */
#define X3D_CONFIG_REPORT_SIZE 256    /* ID, blob zero padded to 255 */
#define X3D_INFO_REPORT_SIZE   32
#define X3D_VENDOR_MAX_AGE_MS  100    /* see DeviceCompletePendingReads */

typedef struct _DEVICE_CONTEXT {
    WDFUSBDEVICE UsbDevice;
    WDFUSBPIPE InterruptPipe;
    WDFQUEUE ReadQueue;               /* manual: pending IOCTL_HID_READ_REPORT */
    WDFWAITLOCK SetFeatureLock;       /* serialises apply + persist of SET_FEATURE */
    HID_DEVICE_ATTRIBUTES HidAttributes; /* written in PrepareHardware only */

    /* Everything below is guarded by Lock: the continuous reader may complete on
     * several threads and the default queue is parallel. */
    WDFSPINLOCK Lock;
    X3D_CONFIG Config;
    X3D_JOY_REPORT LastJoy;
    UCHAR LastRaw[X3D_RAW_SIZE];
    USHORT Seq;
    UCHAR Flags;
    UCHAR LastReadId;
    ULONGLONG LastVendorTick;
} DEVICE_CONTEXT, *PDEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DEVICE_CONTEXT, GetDeviceContext);

VOID DeviceApplyConfig(_In_ PDEVICE_CONTEXT Ctx, _In_ const X3D_CONFIG *Config, _In_ BOOLEAN Persisted);
VOID DeviceOnUsbReport(_In_ PDEVICE_CONTEXT Ctx, _In_reads_(X3D_RAW_SIZE) const UCHAR *Raw);
size_t DeviceBuildInputReport(_In_ PDEVICE_CONTEXT Ctx, _In_ UCHAR ReportId,
                              _Out_writes_bytes_(sizeof(X3D_JOY_REPORT)) UCHAR *Buffer);
VOID DeviceCompletePendingReads(_In_ PDEVICE_CONTEXT Ctx);
