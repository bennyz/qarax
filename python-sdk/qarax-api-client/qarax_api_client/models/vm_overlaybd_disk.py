from __future__ import annotations

from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="VmOverlaybdDisk")


@_attrs_define
class VmOverlaybdDisk:
    """
    Attributes:
        boot_order (int):
        disk_id (str):
        id (UUID):
        image_ref (str):
        registry_url (str):
        vm_id (UUID):
        image_digest (None | str | Unset):
        storage_pool_id (None | Unset | UUID):
    """

    boot_order: int
    disk_id: str
    id: UUID
    image_ref: str
    registry_url: str
    vm_id: UUID
    image_digest: None | str | Unset = UNSET
    storage_pool_id: None | Unset | UUID = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        boot_order = self.boot_order

        disk_id = self.disk_id

        id = str(self.id)

        image_ref = self.image_ref

        registry_url = self.registry_url

        vm_id = str(self.vm_id)

        image_digest: None | str | Unset
        if isinstance(self.image_digest, Unset):
            image_digest = UNSET
        else:
            image_digest = self.image_digest

        storage_pool_id: None | str | Unset
        if isinstance(self.storage_pool_id, Unset):
            storage_pool_id = UNSET
        elif isinstance(self.storage_pool_id, UUID):
            storage_pool_id = str(self.storage_pool_id)
        else:
            storage_pool_id = self.storage_pool_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "boot_order": boot_order,
                "disk_id": disk_id,
                "id": id,
                "image_ref": image_ref,
                "registry_url": registry_url,
                "vm_id": vm_id,
            }
        )
        if image_digest is not UNSET:
            field_dict["image_digest"] = image_digest
        if storage_pool_id is not UNSET:
            field_dict["storage_pool_id"] = storage_pool_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Any) -> T:
        d = dict(src_dict)
        boot_order = d.pop("boot_order")

        disk_id = d.pop("disk_id")

        id = UUID(d.pop("id"))

        image_ref = d.pop("image_ref")

        registry_url = d.pop("registry_url")

        vm_id = UUID(d.pop("vm_id"))

        def _parse_image_digest(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        image_digest = _parse_image_digest(d.pop("image_digest", UNSET))

        def _parse_storage_pool_id(data: object) -> None | Unset | UUID:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                storage_pool_id_type_0 = UUID(data)

                return storage_pool_id_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | Unset | UUID, data)

        storage_pool_id = _parse_storage_pool_id(d.pop("storage_pool_id", UNSET))

        vm_overlaybd_disk = cls(
            boot_order=boot_order,
            disk_id=disk_id,
            id=id,
            image_ref=image_ref,
            registry_url=registry_url,
            vm_id=vm_id,
            image_digest=image_digest,
            storage_pool_id=storage_pool_id,
        )

        vm_overlaybd_disk.additional_properties = d
        return vm_overlaybd_disk

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
