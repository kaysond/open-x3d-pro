/* Persists the active config blob as REG_BINARY "Config". UMDF only grants write
 * access to the driver's own subkey of the device hardware key
 * (PLUGPLAY_REGKEY_DEVICE | WDF_REGKEY_DEVICE_SUBKEY), so that is where it lives. */
#include "driver.h"

static const WCHAR kValueName[] = L"Config";

static NTSTATUS
OpenKey(_In_ WDFDEVICE Device, _In_ ACCESS_MASK Access, _Out_ WDFKEY *Key)
{
    return WdfDeviceOpenRegistryKey(Device, PLUGPLAY_REGKEY_DEVICE | WDF_REGKEY_DEVICE_SUBKEY, Access,
                                    WDF_NO_OBJECT_ATTRIBUTES, Key);
}

static VOID
InitValueName(_Out_ UNICODE_STRING *Name)
{
    Name->Buffer = (PWCH)kValueName;
    Name->Length = (USHORT)(sizeof(kValueName) - sizeof(WCHAR));
    Name->MaximumLength = (USHORT)sizeof(kValueName);
}

NTSTATUS
RegistryLoadConfig(_In_ WDFDEVICE Device, _Out_ X3D_CONFIG *Config)
{
    UCHAR buffer[sizeof(X3D_CONFIG)];
    UNICODE_STRING name;
    WDFKEY key;
    ULONG length = 0, type = 0;
    NTSTATUS status;

    RtlZeroMemory(Config, sizeof(*Config));
    status = OpenKey(Device, KEY_READ, &key);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    InitValueName(&name);
    status = WdfRegistryQueryValue(key, &name, (ULONG)sizeof(buffer), buffer, &length, &type);
    WdfRegistryClose(key);
    if (!NT_SUCCESS(status)) {
        return status; /* absent or larger than a blob */
    }
    if (type != REG_BINARY || !pipeline_validate(buffer, length)) {
        X3D_LOG("stored config invalid, using identity");
        return STATUS_INVALID_PARAMETER;
    }
    RtlCopyMemory(Config, buffer, sizeof(*Config));
    return STATUS_SUCCESS;
}

NTSTATUS
RegistrySaveConfig(_In_ WDFDEVICE Device, _In_ const X3D_CONFIG *Config)
{
    UNICODE_STRING name;
    WDFKEY key;
    NTSTATUS status;

    status = OpenKey(Device, KEY_READ | KEY_SET_VALUE, &key);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    InitValueName(&name);
    status = WdfRegistryAssignValue(key, &name, REG_BINARY, (ULONG)sizeof(X3D_CONFIG), (PVOID)Config);
    WdfRegistryClose(key);
    return status;
}
