use std::convert::Infallible;

use axum::{
    extract::Query,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Deserialize;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tracing::instrument;
use uuid::Uuid;

use crate::model::events;

#[derive(Debug, Deserialize)]
pub struct EventFilter {
    pub vm_id: Option<Uuid>,
    pub status: Option<String>,
    pub tag: Option<String>,
}

#[instrument(skip_all, fields(?filter))]
pub async fn stream(
    Query(filter): Query<EventFilter>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = events::subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        match result {
            Ok(event) => {
                if let Some(ref vm_id) = filter.vm_id
                    && event.vm_id != *vm_id
                {
                    return None;
                }
                if let Some(ref status) = filter.status
                    && event.new_status != *status
                {
                    return None;
                }
                if let Some(ref tag) = filter.tag
                    && !event.tags.contains(tag)
                {
                    return None;
                }

                let data = serde_json::to_string(&event).ok()?;
                let sse_event = Event::default()
                    .event(&event.event)
                    .id(event.vm_id.to_string())
                    .data(data);
                Some(Ok(sse_event))
            }
            // Lagged — skip missed events and continue
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
