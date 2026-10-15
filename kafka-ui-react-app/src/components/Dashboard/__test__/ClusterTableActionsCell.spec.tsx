import React from 'react';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import fetchMock from 'fetch-mock';
import { CellContext } from '@tanstack/react-table';
import { Cluster } from 'generated-sources';
import { render } from 'lib/testHelpers';
import ClusterTableActionsCell from 'components/Dashboard/ClusterTableActionsCell';

const currentConfig = {
  properties: {
    kafka: {
      clusters: [
        { name: 'local', bootstrapServers: 'localhost:9092' },
        { name: 'staging', bootstrapServers: 'staging:9092' },
      ],
    },
  },
};

describe('ClusterTableActionsCell', () => {
  const renderComponent = () => {
    const props = {
      row: { original: { name: 'local' } },
    } as CellContext<Cluster, unknown>;
    render(<ClusterTableActionsCell {...props} />);
  };

  beforeEach(() => {
    fetchMock.get('/api/authorization', { rbacEnabled: false });
  });
  afterEach(() => fetchMock.restore());

  it('renders Configure and Delete buttons', () => {
    renderComponent();
    expect(screen.getByText('Configure')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument();
  });

  it('asks for confirmation before deleting', async () => {
    renderComponent();
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(screen.getByRole('dialog')).toHaveTextContent(
      'Are you sure you want to remove local cluster?'
    );
    expect(fetchMock.called('/api/config')).toBeFalsy();
  });

  it('saves the config without the deleted cluster on confirm', async () => {
    fetchMock.getOnce('/api/config', currentConfig);
    const putMock = fetchMock.putOnce('/api/config', 200);
    renderComponent();

    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await userEvent.click(screen.getByRole('button', { name: 'Confirm' }));

    await waitFor(() => expect(putMock.called()).toBeTruthy());
    const body = JSON.parse(String(putMock.lastOptions()?.body));
    expect(body.config.properties.kafka.clusters).toEqual([
      { name: 'staging', bootstrapServers: 'staging:9092' },
    ]);
  });
});
