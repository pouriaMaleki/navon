use embedded_hal::i2c::I2c;

use crate::{
    board_config::{TOUCH_I2C_ADDR_PRIMARY, TOUCH_I2C_ADDR_SECONDARY},
    gestures::{GestureRecognizer, GestureUpdate, TouchFrame, TouchPoint},
};

const GT9271_REG_PRODUCT_ID: u16 = 0x8140;
const GT9271_REG_STATUS: u16 = 0x814E;
const GT9271_REG_POINTS: u16 = 0x8150;
const GT9271_POINT_STRIDE: usize = 8;
const GT9271_MAX_CONTACTS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchPanelConfig {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchError<E> {
    Bus(E),
    ProductIdMismatch([u8; 4]),
    NoControllerDetected,
}

pub struct TouchInput<I2C> {
    i2c: I2C,
    address: u8,
    recognizer: GestureRecognizer,
}

impl<I2C, E> TouchInput<I2C>
where
    I2C: I2c<Error = E>,
{
    pub fn new_bus_only(mut i2c: I2C, config: TouchPanelConfig) -> Result<Self, TouchError<E>> {
        let address = detect_address(&mut i2c)?;
        let mut this = Self {
            i2c,
            address,
            recognizer: GestureRecognizer::new(config.width, config.height),
        };
        this.validate_product_id()?;
        Ok(this)
    }

    pub fn poll(&mut self, now_ms: u32) -> Result<GestureUpdate, TouchError<E>> {
        let frame = self.read_frame()?;
        Ok(self.recognizer.update(frame, now_ms))
    }

    fn read_frame(&mut self) -> Result<TouchFrame, TouchError<E>> {
        let mut status = [0_u8; 1];
        self.read_register(GT9271_REG_STATUS, &mut status)?;
        if (status[0] & 0x80) == 0 {
            return Ok(TouchFrame::empty());
        }

        let count = (status[0] & 0x0F).min(2) as usize;
        let mut frame = TouchFrame::empty();
        if count > 0 {
            let mut raw = [0_u8; GT9271_POINT_STRIDE * 2];
            self.read_register(GT9271_REG_POINTS, &mut raw[..count * GT9271_POINT_STRIDE])?;
            for idx in 0..count {
                let base = idx * GT9271_POINT_STRIDE;
                frame.points[idx] = Some(TouchPoint {
                    id: raw[base] & 0x0F,
                    x: u16::from_le_bytes([raw[base + 1], raw[base + 2]]),
                    y: u16::from_le_bytes([raw[base + 3], raw[base + 4]]),
                });
            }
        }

        self.write_register(GT9271_REG_STATUS, &[0])?;
        Ok(frame)
    }

    fn validate_product_id(&mut self) -> Result<(), TouchError<E>> {
        let mut id = [0_u8; 4];
        self.read_register(GT9271_REG_PRODUCT_ID, &mut id)?;
        if is_valid_product_id(id) {
            Ok(())
        } else {
            Err(TouchError::ProductIdMismatch(id))
        }
    }

    fn read_register(&mut self, register: u16, buffer: &mut [u8]) -> Result<(), TouchError<E>> {
        self.i2c
            .write_read(self.address, &register.to_be_bytes(), buffer)
            .map_err(TouchError::Bus)
    }

    fn write_register(&mut self, register: u16, data: &[u8]) -> Result<(), TouchError<E>> {
        let mut payload = [0_u8; 2 + GT9271_MAX_CONTACTS * GT9271_POINT_STRIDE];
        let total = 2 + data.len();
        payload[..2].copy_from_slice(&register.to_be_bytes());
        payload[2..total].copy_from_slice(data);
        self.i2c
            .write(self.address, &payload[..total])
            .map_err(TouchError::Bus)
    }
}

fn detect_address<I2C, E>(i2c: &mut I2C) -> Result<u8, TouchError<E>>
where
    I2C: I2c<Error = E>,
{
    for address in [TOUCH_I2C_ADDR_PRIMARY, TOUCH_I2C_ADDR_SECONDARY] {
        let mut id = [0_u8; 4];
        if i2c
            .write_read(address, &GT9271_REG_PRODUCT_ID.to_be_bytes(), &mut id)
            .is_ok()
            && is_valid_product_id(id)
        {
            return Ok(address);
        }
    }
    Err(TouchError::NoControllerDetected)
}

fn is_valid_product_id(id: [u8; 4]) -> bool {
    id[0].is_ascii_digit() && id[1].is_ascii_digit() && id[2].is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};

    struct FakeI2c {
        next_status: u8,
        points: [u8; 16],
    }

    impl ErrorType for FakeI2c {
        type Error = ErrorKind;
    }

    impl I2c for FakeI2c {
        fn transaction(
            &mut self,
            _address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            if let [Operation::Write(register), Operation::Read(buffer)] = operations {
                let reg = u16::from_be_bytes([register[0], register[1]]);
                match reg {
                    GT9271_REG_PRODUCT_ID => {
                        buffer.copy_from_slice(b"9271");
                    }
                    GT9271_REG_STATUS => {
                        buffer[0] = self.next_status;
                    }
                    GT9271_REG_POINTS => {
                        buffer.copy_from_slice(&self.points[..buffer.len()]);
                    }
                    _ => {}
                }
                return Ok(());
            }

            if let [Operation::Write(_buffer)] = operations {
                return Ok(());
            }

            Err(ErrorKind::Other)
        }
    }

    #[test]
    fn reads_touch_frame_and_recognizes_tap() {
        let mut touch = TouchInput::new_bus_only(
            FakeI2c {
                next_status: 0x81,
                points: [1, 0x80, 0x02, 0x20, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
            TouchPanelConfig {
                width: 800,
                height: 800,
            },
        )
        .expect("touch controller");

        let down = touch.poll(10).expect("touch poll");
        assert_eq!(down.tap, None);
    }
}
