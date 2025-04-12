trait Event {
    type Id: Eq;
    type Version: Eq + Ord;

    fn id(&self) -> Self::Id;
    fn version(&self) -> Self::Version;
}

trait Aggregate: Sized {
    type Error: std::error::Error;
    type Event: Event<Id = Self::Id, Version = Self::Version>;
    type Id: Eq;
    type Version: Eq + Ord;

    fn replay<I>(events: I) -> Result<Self, Self::Error>
    where
        I: IntoIterator<Item = Self::Event>;

    fn id(&self) -> Self::Id;
    fn version(&self) -> Self::Version;
}

trait Repository {
    type Aggregate: Aggregate;
    type Error: std::error::Error;

    async fn find(
        &self,
        id: &<Self::Aggregate as Aggregate>::Id,
    ) -> Result<Option<Self::Aggregate>, Self::Error>;

    async fn store(
        &self,
        id: &<Self::Aggregate as Aggregate>::Id,
        expected_version: Option<&<Self::Aggregate as Aggregate>::Version>,
        new_events: &[<Self::Aggregate as Aggregate>::Event],
    ) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    enum AggregateEvent {
        Created(AggregateCreated),
        Updated(AggregateUpdated),
    }

    impl Event for AggregateEvent {
        type Id = AggregateId;

        type Version = AggregateVersion;

        fn id(&self) -> Self::Id {
            AggregateId(
                match self {
                    AggregateEvent::Created(AggregateCreated { id, .. }) => id,
                    AggregateEvent::Updated(AggregateUpdated { id, .. }) => id,
                }
                .to_owned(),
            )
        }

        fn version(&self) -> Self::Version {
            AggregateVersion(*match self {
                AggregateEvent::Created(AggregateCreated { version, .. }) => version,
                AggregateEvent::Updated(AggregateUpdated { version, .. }) => version,
            })
        }
    }

    #[derive(Clone)]
    struct AggregateCreated {
        id: String,
        version: u16,
    }

    #[derive(Clone)]
    struct AggregateUpdated {
        id: String,
        version: u16,
    }

    #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
    struct AggregateId(String);

    #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
    struct AggregateVersion(u16);

    struct AggregateImpl {
        id: AggregateId,
        version: AggregateVersion,
    }

    impl AggregateImpl {
        fn create() -> Self {
            Self {
                id: AggregateId("1".to_owned()),
                version: AggregateVersion(1),
            }
        }

        fn update(&self) -> Result<(Self, Vec<AggregateEvent>), std::io::Error> {
            let new_version = self.version.0 + 1;
            let event = AggregateEvent::Updated(AggregateUpdated {
                id: self.id.0.clone(),
                version: new_version,
            });
            Ok((
                Self {
                    id: self.id.clone(),
                    version: AggregateVersion(new_version),
                },
                vec![event],
            ))
        }
    }

    impl Aggregate for AggregateImpl {
        type Error = std::io::Error;
        type Event = AggregateEvent;
        type Id = AggregateId;
        type Version = AggregateVersion;

        fn replay<I>(events: I) -> Result<Self, Self::Error>
        where
            I: IntoIterator<Item = Self::Event>,
        {
            let mut iter = events.into_iter();
            let mut aggregate = match iter.next() {
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "No events provided",
                )),
                Some(event) => match event {
                    AggregateEvent::Created(AggregateCreated { id, version }) => Ok(Self {
                        id: AggregateId(id),
                        version: AggregateVersion(version),
                    }),
                    _ => Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Invalid event",
                    )),
                },
            }?;
            for event in iter {
                match event {
                    AggregateEvent::Created(_) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Invalid event",
                        ));
                    }
                    AggregateEvent::Updated(_) => {
                        aggregate.version = event.version();
                    }
                }
            }
            Ok(aggregate)
        }

        fn id(&self) -> Self::Id {
            self.id.clone()
        }

        fn version(&self) -> Self::Version {
            self.version.clone()
        }
    }

    struct RepositoryImpl {
        aggregates: std::sync::Arc<std::sync::Mutex<Vec<(AggregateId, AggregateVersion)>>>,
        events: std::sync::Arc<std::sync::Mutex<Vec<(AggregateId, Vec<AggregateEvent>)>>>,
    }

    impl Repository for RepositoryImpl {
        type Aggregate = AggregateImpl;
        type Error = std::io::Error;

        async fn find(
            &self,
            id: &<Self::Aggregate as Aggregate>::Id,
        ) -> Result<Option<Self::Aggregate>, Self::Error> {
            let aggregates = self.aggregates.lock().unwrap();
            match aggregates.iter().find(|it| it.0 == *id) {
                None => return Ok(None),
                Some(_) => {
                    let events = self.events.lock().unwrap();
                    let events = match events.iter().find(|it| it.0 == *id) {
                        None => return Ok(None),
                        Some((_, events)) => events,
                    };
                    Self::Aggregate::replay(events.clone()).map(Some)
                }
            }
        }

        async fn store(
            &self,
            id: &<Self::Aggregate as Aggregate>::Id,
            expected_version: Option<&<Self::Aggregate as Aggregate>::Version>,
            new_events: &[<Self::Aggregate as Aggregate>::Event],
        ) -> Result<(), Self::Error> {
            let last_event = match new_events.last() {
                None => return Ok(()),
                Some(event) => event,
            };

            let mut aggregates = self.aggregates.lock().unwrap();
            match expected_version {
                None => {
                    // create
                    if aggregates.iter().any(|it| &it.0 == id) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Aggregate already exists",
                        ));
                    }
                    aggregates.push((last_event.id(), last_event.version()));
                }
                Some(expected_version) => {
                    // update
                    let found = aggregates.iter_mut().find(|it| &it.0 == id);
                    match found {
                        Some(it) if it.1 == *expected_version => {
                            it.1 = last_event.version();
                        }
                        None | Some(_) => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Version mismatch",
                            ));
                        }
                    }
                }
            }

            let mut events = self.events.lock().unwrap();
            if events.iter().all(|it| it.0 != *id) {
                events.push((id.clone(), vec![]));
            }
            let (_, events) = events
                .iter_mut()
                .find(|it| it.0 == *id)
                .expect("events to exist");
            for new_event in new_events {
                events.push(new_event.clone());
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_aggregate() {
        let aggregate = AggregateImpl::create();
        assert_eq!(aggregate.id(), AggregateId("1".to_owned()));
        assert_eq!(aggregate.version(), AggregateVersion(1));
    }

    #[tokio::test]
    async fn test_repository() {
        let repository = RepositoryImpl {
            aggregates: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
            events: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
        };

        let aggregate = AggregateImpl::create();
        let id = aggregate.id().clone();
        let version = aggregate.version().clone();

        assert!(repository.find(&id).await.unwrap().is_none());

        repository
            .store(
                &id,
                None,
                &[AggregateEvent::Created(AggregateCreated {
                    id: id.0.clone(),
                    version: version.0,
                })],
            )
            .await
            .unwrap();

        let found_aggregate = repository.find(&id).await.unwrap().unwrap();
        assert_eq!(found_aggregate.id(), id);
        assert_eq!(found_aggregate.version(), version);

        let (updated_aggregate, events) = found_aggregate.update().unwrap();

        repository
            .store(&id, Some(&found_aggregate.version()), &events)
            .await
            .unwrap();

        let found_aggregate = repository.find(&id).await.unwrap().unwrap();
        assert_eq!(found_aggregate.id(), id);
        assert_eq!(found_aggregate.version(), updated_aggregate.version());
    }
}
