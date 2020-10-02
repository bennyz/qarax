# coding: utf-8

"""
    Qarax API

    The API for Qarax manager  # noqa: E501

    OpenAPI spec version: 0.1.0
    
    Generated by: https://github.com/swagger-api/swagger-codegen.git
"""

import pprint
import re  # noqa: F401

import six

class Drive(object):
    """NOTE: This class is auto generated by the swagger code generator program.

    Do not edit the class manually.
    """
    """
    Attributes:
      swagger_types (dict): The key is attribute name
                            and the value is attribute type.
      attribute_map (dict): The key is attribute name
                            and the value is json key in definition.
    """
    swagger_types = {
        'id': 'str',
        'name': 'str',
        'readonly': 'bool',
        'rootfs': 'bool',
        'status': 'str',
        'storage_id': 'str'
    }

    attribute_map = {
        'id': 'id',
        'name': 'name',
        'readonly': 'readonly',
        'rootfs': 'rootfs',
        'status': 'status',
        'storage_id': 'storage_id'
    }

    def __init__(self, id=None, name=None, readonly=None, rootfs=None, status=None, storage_id=None):  # noqa: E501
        """Drive - a model defined in Swagger"""  # noqa: E501
        self._id = None
        self._name = None
        self._readonly = None
        self._rootfs = None
        self._status = None
        self._storage_id = None
        self.discriminator = None
        if id is not None:
            self.id = id
        if name is not None:
            self.name = name
        if readonly is not None:
            self.readonly = readonly
        if rootfs is not None:
            self.rootfs = rootfs
        if status is not None:
            self.status = status
        if storage_id is not None:
            self.storage_id = storage_id

    @property
    def id(self):
        """Gets the id of this Drive.  # noqa: E501


        :return: The id of this Drive.  # noqa: E501
        :rtype: str
        """
        return self._id

    @id.setter
    def id(self, id):
        """Sets the id of this Drive.


        :param id: The id of this Drive.  # noqa: E501
        :type: str
        """

        self._id = id

    @property
    def name(self):
        """Gets the name of this Drive.  # noqa: E501


        :return: The name of this Drive.  # noqa: E501
        :rtype: str
        """
        return self._name

    @name.setter
    def name(self, name):
        """Sets the name of this Drive.


        :param name: The name of this Drive.  # noqa: E501
        :type: str
        """

        self._name = name

    @property
    def readonly(self):
        """Gets the readonly of this Drive.  # noqa: E501


        :return: The readonly of this Drive.  # noqa: E501
        :rtype: bool
        """
        return self._readonly

    @readonly.setter
    def readonly(self, readonly):
        """Sets the readonly of this Drive.


        :param readonly: The readonly of this Drive.  # noqa: E501
        :type: bool
        """

        self._readonly = readonly

    @property
    def rootfs(self):
        """Gets the rootfs of this Drive.  # noqa: E501


        :return: The rootfs of this Drive.  # noqa: E501
        :rtype: bool
        """
        return self._rootfs

    @rootfs.setter
    def rootfs(self, rootfs):
        """Sets the rootfs of this Drive.


        :param rootfs: The rootfs of this Drive.  # noqa: E501
        :type: bool
        """

        self._rootfs = rootfs

    @property
    def status(self):
        """Gets the status of this Drive.  # noqa: E501


        :return: The status of this Drive.  # noqa: E501
        :rtype: str
        """
        return self._status

    @status.setter
    def status(self, status):
        """Sets the status of this Drive.


        :param status: The status of this Drive.  # noqa: E501
        :type: str
        """

        self._status = status

    @property
    def storage_id(self):
        """Gets the storage_id of this Drive.  # noqa: E501


        :return: The storage_id of this Drive.  # noqa: E501
        :rtype: str
        """
        return self._storage_id

    @storage_id.setter
    def storage_id(self, storage_id):
        """Sets the storage_id of this Drive.


        :param storage_id: The storage_id of this Drive.  # noqa: E501
        :type: str
        """

        self._storage_id = storage_id

    def to_dict(self):
        """Returns the model properties as a dict"""
        result = {}

        for attr, _ in six.iteritems(self.swagger_types):
            value = getattr(self, attr)
            if isinstance(value, list):
                result[attr] = list(map(
                    lambda x: x.to_dict() if hasattr(x, "to_dict") else x,
                    value
                ))
            elif hasattr(value, "to_dict"):
                result[attr] = value.to_dict()
            elif isinstance(value, dict):
                result[attr] = dict(map(
                    lambda item: (item[0], item[1].to_dict())
                    if hasattr(item[1], "to_dict") else item,
                    value.items()
                ))
            else:
                result[attr] = value
        if issubclass(Drive, dict):
            for key, value in self.items():
                result[key] = value

        return result

    def to_str(self):
        """Returns the string representation of the model"""
        return pprint.pformat(self.to_dict())

    def __repr__(self):
        """For `print` and `pprint`"""
        return self.to_str()

    def __eq__(self, other):
        """Returns true if both objects are equal"""
        if not isinstance(other, Drive):
            return False

        return self.__dict__ == other.__dict__

    def __ne__(self, other):
        """Returns true if both objects are not equal"""
        return not self == other