from __future__ import annotations

from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="NewVmFilesystem")


@_attrs_define
class NewVmFilesystem:
    """
    Attributes:
        tag (str):
        vm_id (UUID):
        image_digest (None | str | Unset):
        image_ref (None | str | Unset):
        num_queues (int | None | Unset):
        pci_segment (int | None | Unset):
        queue_size (int | None | Unset):
    """

    tag: str
    vm_id: UUID
    image_digest: None | str | Unset = UNSET
    image_ref: None | str | Unset = UNSET
    num_queues: int | None | Unset = UNSET
    pci_segment: int | None | Unset = UNSET
    queue_size: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        tag = self.tag

        vm_id = str(self.vm_id)

        image_digest: None | str | Unset
        if isinstance(self.image_digest, Unset):
            image_digest = UNSET
        else:
            image_digest = self.image_digest

        image_ref: None | str | Unset
        if isinstance(self.image_ref, Unset):
            image_ref = UNSET
        else:
            image_ref = self.image_ref

        num_queues: int | None | Unset
        if isinstance(self.num_queues, Unset):
            num_queues = UNSET
        else:
            num_queues = self.num_queues

        pci_segment: int | None | Unset
        if isinstance(self.pci_segment, Unset):
            pci_segment = UNSET
        else:
            pci_segment = self.pci_segment

        queue_size: int | None | Unset
        if isinstance(self.queue_size, Unset):
            queue_size = UNSET
        else:
            queue_size = self.queue_size

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "tag": tag,
                "vm_id": vm_id,
            }
        )
        if image_digest is not UNSET:
            field_dict["image_digest"] = image_digest
        if image_ref is not UNSET:
            field_dict["image_ref"] = image_ref
        if num_queues is not UNSET:
            field_dict["num_queues"] = num_queues
        if pci_segment is not UNSET:
            field_dict["pci_segment"] = pci_segment
        if queue_size is not UNSET:
            field_dict["queue_size"] = queue_size

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Any) -> T:
        d = dict(src_dict)
        tag = d.pop("tag")

        vm_id = UUID(d.pop("vm_id"))

        def _parse_image_digest(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        image_digest = _parse_image_digest(d.pop("image_digest", UNSET))

        def _parse_image_ref(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        image_ref = _parse_image_ref(d.pop("image_ref", UNSET))

        def _parse_num_queues(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        num_queues = _parse_num_queues(d.pop("num_queues", UNSET))

        def _parse_pci_segment(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        pci_segment = _parse_pci_segment(d.pop("pci_segment", UNSET))

        def _parse_queue_size(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        queue_size = _parse_queue_size(d.pop("queue_size", UNSET))

        new_vm_filesystem = cls(
            tag=tag,
            vm_id=vm_id,
            image_digest=image_digest,
            image_ref=image_ref,
            num_queues=num_queues,
            pci_segment=pci_segment,
            queue_size=queue_size,
        )

        new_vm_filesystem.additional_properties = d
        return new_vm_filesystem

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
