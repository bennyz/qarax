from enum import Enum


class JobType(str, Enum):
    IMAGE_PULL = "image_pull"

    def __str__(self) -> str:
        return str(self.value)
