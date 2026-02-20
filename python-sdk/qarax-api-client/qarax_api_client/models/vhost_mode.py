from enum import Enum


class VhostMode(str, Enum):
    CLIENT = "client"
    SERVER = "server"

    def __str__(self) -> str:
        return str(self.value)
