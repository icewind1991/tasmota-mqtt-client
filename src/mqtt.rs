use crate::Result;
use async_stream::try_stream;
use rumqttc::{matches, AsyncClient, Event, EventLoop, MqttOptions, Packet, Publish, QoS};
use serde::Serialize;
use std::pin::pin;
use std::sync::Arc;
use tokio::spawn;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, error};

pub struct MqttHelper {
    client: AsyncClient,
    #[allow(clippy::type_complexity)]
    listeners: Arc<Mutex<Vec<(String, Sender<Publish>)>>>,
}

impl MqttHelper {
    pub fn connect(opts: MqttOptions) -> Self {
        let (client, event_loop) = AsyncClient::new(opts, 10);

        let listeners = Arc::<Mutex<Vec<(String, Sender<_>)>>>::default();
        let senders = listeners.clone();

        spawn(async move {
            let stream = event_loop_to_stream(event_loop);
            let messages = stream
                .filter_map(|event| match event {
                    Ok(event) => {
                        debug!(event = ?event, "processing event");
                        Some(event)
                    }
                    Err(e) => {
                        error!(error = ?e, "error while receiving mqtt message");
                        None
                    }
                })
                .filter_map(|event| match event {
                    Event::Incoming(Packet::Publish(message)) => Some(message),
                    _ => None,
                });

            let mut messages = pin!(messages);

            while let Some(message) = messages.next().await {
                let message: Publish = message;
                let mut listeners_ref = senders.lock().await;
                listeners_ref.retain(|(_, sender)| !sender.is_closed());
                for (filter, sender) in listeners_ref.iter() {
                    if matches(&message.topic, filter.as_str()) {
                        let _ = sender.send(message.clone()).await;
                    }
                }
            }
        });

        Self { client, listeners }
    }

    pub async fn send<B: Serialize>(&self, topic: &str, body: &B) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, false, serde_json::to_vec(body)?)
            .await?;
        Ok(())
    }

    pub async fn send_str(&self, topic: &str, body: &str) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, false, body)
            .await?;
        Ok(())
    }

    pub async fn subscribe(&self, topic: String) -> Result<Receiver<Publish>> {
        self.client.subscribe(&topic, QoS::AtLeastOnce).await?;
        let (tx, rx) = channel(10);
        self.listeners.lock().await.push((topic, tx));
        Ok(rx)
    }
}

fn event_loop_to_stream(mut event_loop: EventLoop) -> impl Stream<Item = Result<Event>> {
    try_stream! {
        loop {
            let event = event_loop.poll().await?;
            yield event;
        }
    }
}
