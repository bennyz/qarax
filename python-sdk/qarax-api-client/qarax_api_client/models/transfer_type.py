from enum import Enum


class TransferType(str, Enum):
    DOWNLOAD = "download"
    LOCAL_COPY = "local_copy"

    def __str__(self) -> str:
        return str(self.value)
