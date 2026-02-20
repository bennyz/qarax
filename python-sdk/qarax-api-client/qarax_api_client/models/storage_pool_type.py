from enum import Enum


class StoragePoolType(str, Enum):
    LOCAL = "local"
    NFS = "nfs"

    def __str__(self) -> str:
        return str(self.value)
