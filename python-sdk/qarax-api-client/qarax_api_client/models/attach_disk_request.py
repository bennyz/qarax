from __future__ import annotations

from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AttachDiskRequest")


@_attrs_define
class AttachDiskRequest:
    """
    Attributes:
        storage_object_id (UUID): Storage object ID (must be `oci_image` type).
        boot_order (int | None | Unset): Boot priority — lower is higher priority (default: `0`).
        disk_id (None | str | Unset): Disk identifier inside the VM (default: `"vda"`).
    """

    storage_object_id: UUID
    boot_order: int | None | Unset = UNSET
    disk_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        storage_object_id = str(self.storage_object_id)

        boot_order: int | None | Unset
        if isinstance(self.boot_order, Unset):
            boot_order = UNSET
        else:
            boot_order = self.boot_order

        disk_id: None | str | Unset
        if isinstance(self.disk_id, Unset):
            disk_id = UNSET
        else:
            disk_id = self.disk_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "storage_object_id": storage_object_id,
            }
        )
        if boot_order is not UNSET:
            field_dict["boot_order"] = boot_order
        if disk_id is not UNSET:
            field_dict["disk_id"] = disk_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Any) -> T:
        d = dict(src_dict)
        storage_object_id = UUID(d.pop("storage_object_id"))

        def _parse_boot_order(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        boot_order = _parse_boot_order(d.pop("boot_order", UNSET))

        def _parse_disk_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        disk_id = _parse_disk_id(d.pop("disk_id", UNSET))

        attach_disk_request = cls(
            storage_object_id=storage_object_id,
            boot_order=boot_order,
            disk_id=disk_id,
        )

        attach_disk_request.additional_properties = d
        return attach_disk_request

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
