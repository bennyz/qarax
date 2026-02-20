from enum import Enum


class StorageObjectType(str, Enum):
    DISK = "disk"
    INITRD = "initrd"
    ISO = "iso"
    KERNEL = "kernel"
    SNAPSHOT = "snapshot"

    def __str__(self) -> str:
        return str(self.value)
