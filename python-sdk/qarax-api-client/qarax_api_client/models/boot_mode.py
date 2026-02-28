from enum import Enum


class BootMode(str, Enum):
    FIRMWARE = "firmware"
    KERNEL = "kernel"

    def __str__(self) -> str:
        return str(self.value)
