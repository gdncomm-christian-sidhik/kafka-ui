import React, { useMemo } from 'react';
import { Cluster, ResourceType } from 'generated-sources';
import { CellContext } from '@tanstack/react-table';
import { clusterConfigPath } from 'lib/paths';
import { useGetUserInfo } from 'lib/hooks/api/roles';
import { useDeleteClusterFromAppConfig } from 'lib/hooks/api/appConfig';
import { useConfirm } from 'lib/hooks/useConfirm';
import { showAlert } from 'lib/errorHandling';
import { ActionCanButton } from 'components/common/ActionComponent';

import * as S from './Dashboard.styled';

type Props = CellContext<Cluster, unknown>;

const ClusterTableActionsCell: React.FC<Props> = ({ row }) => {
  const { name } = row.original;
  const { data } = useGetUserInfo();
  const confirm = useConfirm(true);
  const deleteCluster = useDeleteClusterFromAppConfig();

  const hasPermissions = useMemo(() => {
    if (!data?.rbacEnabled) return true;
    return !!data?.userInfo?.permissions.some(
      (permission) => permission.resource === ResourceType.APPLICATIONCONFIG
    );
  }, [data]);

  const handleDelete = () =>
    confirm(
      <>
        Are you sure you want to remove <b>{name}</b> cluster? The application
        will restart to apply the change.
      </>,
      async () => {
        try {
          await deleteCluster.mutateAsync(name);
        } catch (e) {
          showAlert('error', {
            id: 'app-config-update-error',
            title: 'Error updating application config',
            message: `There was an error removing ${name} cluster`,
          });
        }
      }
    );

  return (
    <S.ActionsCell>
      <ActionCanButton
        buttonType="secondary"
        buttonSize="S"
        to={clusterConfigPath(name)}
        canDoAction={hasPermissions}
      >
        Configure
      </ActionCanButton>
      <ActionCanButton
        buttonType="danger"
        buttonSize="S"
        onClick={handleDelete}
        canDoAction={hasPermissions}
        inProgress={deleteCluster.isLoading}
      >
        Delete
      </ActionCanButton>
    </S.ActionsCell>
  );
};

export default ClusterTableActionsCell;
