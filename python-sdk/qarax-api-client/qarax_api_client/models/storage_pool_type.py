from enum import Enum


class StoragePoolType(str, Enum):
    LOCAL = "local"
    NFS = "nfs"
    OVERLAY_BD = "overlay_bd"

    def __str__(self) -> str:
        return str(self.value)
