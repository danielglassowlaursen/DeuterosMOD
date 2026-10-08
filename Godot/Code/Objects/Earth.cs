using Godot;
using System;
using System.Formats.Asn1;
using static Deuteros.Code.Enums;

namespace Deuteros.Code.Objects
{
    [Serializable]
    public partial class Earth : Planet, Interfaces.IPlanet
    {
        public Staff ResearchStaff { get; set; }
        public Factory Factory { get; set; }
        public Objects.Training TrainingData { get; set; }
        public bool GroundSelected { get; set; }
        public ResearchItem CurrentResearchItem { get; set; }

        public Earth(Enums.StellarBodies planetId, int order) : base(planetId, order)
        {
            Deuteros.Code.GameCore.SingletonInstance.DayPassed += DayTick;
            GroundSelected = true;
            Factory = new Factory();
        }

        public override void AddItems(Enums.ItemTypes itemToAdd, int count)
        {
            if (GroundSelected)
                PlanetResources.Stores[itemToAdd] += count;
            else
                Station.Resources.Stores[itemToAdd] += count;
        }

        //Triggered from gamecore
        public new void DayTick(uint previousDay, uint currentDay)
        {
            TrainingData.ChildDayTick(previousDay, currentDay);
        }
    }
}