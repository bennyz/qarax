from __future__ import annotations

from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.host_status import HostStatus
from ..types import UNSET, Unset

T = TypeVar("T", bound="Host")


@_attrs_define
class Host:
    """
    Attributes:
        address (str):
        host_user (str):
        id (UUID):
        name (str):
        port (int):
        status (HostStatus):
        cloud_hypervisor_version (None | str | Unset):
        kernel_version (None | str | Unset):
    """

    address: str
    host_user: str
    id: UUID
    name: str
    port: int
    status: HostStatus
    cloud_hypervisor_version: None | str | Unset = UNSET
    kernel_version: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        address = self.address

        host_user = self.host_user

        id = str(self.id)

        name = self.name

        port = self.port

        status = self.status.value

        cloud_hypervisor_version: None | str | Unset
        if isinstance(self.cloud_hypervisor_version, Unset):
            cloud_hypervisor_version = UNSET
        else:
            cloud_hypervisor_version = self.cloud_hypervisor_version

        kernel_version: None | str | Unset
        if isinstance(self.kernel_version, Unset):
            kernel_version = UNSET
        else:
            kernel_version = self.kernel_version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "address": address,
                "host_user": host_user,
                "id": id,
                "name": name,
                "port": port,
                "status": status,
            }
        )
        if cloud_hypervisor_version is not UNSET:
            field_dict["cloud_hypervisor_version"] = cloud_hypervisor_version
        if kernel_version is not UNSET:
            field_dict["kernel_version"] = kernel_version

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Any) -> T:
        d = dict(src_dict)
        address = d.pop("address")

        host_user = d.pop("host_user")

        id = UUID(d.pop("id"))

        name = d.pop("name")

        port = d.pop("port")

        status = HostStatus(d.pop("status"))

        def _parse_cloud_hypervisor_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        cloud_hypervisor_version = _parse_cloud_hypervisor_version(d.pop("cloud_hypervisor_version", UNSET))

        def _parse_kernel_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        kernel_version = _parse_kernel_version(d.pop("kernel_version", UNSET))

        host = cls(
            address=address,
            host_user=host_user,
            id=id,
            name=name,
            port=port,
            status=status,
            cloud_hypervisor_version=cloud_hypervisor_version,
            kernel_version=kernel_version,
        )

        host.additional_properties = d
        return host

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
